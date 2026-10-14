#include "pulsebeam-webrtc-sys/native/async_dns.h"
#include "pulsebeam-webrtc-sys/native/socket_address.h"
#include "pulsebeam-webrtc-sys/src/lib.rs.h"
#include "api/task_queue/task_queue_base.h"
#include <mutex>
#include <utility>
#include <vector>

namespace pulsebeam::webrtc_sys {
namespace {
using Callback = absl::AnyInvocable<void()>;
struct ResolverScope : std::enable_shared_from_this<ResolverScope> {
  std::shared_ptr<NativeDnsResolutionCompletion::Request> Begin(Callback callback);
  void Retire();
  std::mutex mutex;
  // Borrowed only until product destruction on this exact sequence. Native
  // owners must destroy sequence-affine products before deleting their queue;
  // peer ownership retains factory role queues through native peer teardown.
  webrtc::TaskQueueBase* queue = nullptr;
  bool retired = false;
  std::shared_ptr<NativeDnsResolutionCompletion::Request> pending;
};
}
struct NativeDnsResolutionCompletion::Request : std::enable_shared_from_this<Request> {
  Request(std::shared_ptr<ResolverScope> scope, Callback callback)
      : scope(std::move(scope)), callback(std::move(callback)) {}
  std::uint8_t Complete() {
    std::lock_guard lock(scope->mutex);
    if (scope->retired) return 1;
    if (scope->pending.get() != this) return 2;
    if (!callback || submitted) return 3;
    if (!scope->queue) return 4;
    submitted = true;
    scope->queue->PostTask([request = shared_from_this()] { request->Run(); });
    return 0;
  }
  void Discard() {
    std::lock_guard lock(scope->mutex);
    if (scope->retired || !callback || submitted) return;
    // In the supported native path a current queue exists. Without one, no
    // arbitrary Rust thread may release sequence-affine captures. Retain the
    // request on its product until same-sequence retirement instead.
    if (!scope->queue) return;
    submitted = true;
    scope->queue->PostTask([request = shared_from_this()] { request->Cancel(); });
  }
  void Cancel() {
    Callback discarded;
    {
      std::lock_guard lock(scope->mutex);
      discarded = std::move(callback);
    }
    discarded = nullptr;
  }
  void Run() {
    Callback pending;
    {
      std::lock_guard lock(scope->mutex);
      if (scope->retired || scope->pending.get() != this || !callback) return;
      pending = std::move(callback);
    }
    pending();
    // Invocation may delete the product. Access only this request and locals.
    pending = nullptr;
  }
  std::shared_ptr<ResolverScope> scope;
  Callback callback;
  bool submitted = false;
};
namespace {
std::shared_ptr<NativeDnsResolutionCompletion::Request> ResolverScope::Begin(Callback callback) {
  auto request = std::make_shared<NativeDnsResolutionCompletion::Request>(shared_from_this(), std::move(callback));
  Callback discarded;
  {
    std::lock_guard lock(mutex);
    if (retired) {
      discarded = std::move(request->callback);
    } else {
      if (pending) discarded = std::move(pending->callback);
      queue = webrtc::TaskQueueBase::Current();
      pending = request;
    }
  }
  discarded = nullptr;
  return request;
}
void ResolverScope::Retire() {
  Callback discarded;
  {
    std::lock_guard lock(mutex);
    retired = true;
    queue = nullptr;
    if (pending) discarded = std::move(pending->callback);
    pending.reset();
  }
  discarded = nullptr;
}
struct ResolverSource {
  explicit ResolverSource(rust::Box<RustAsyncDnsResolver> source) : source(std::move(source)) {}
  rust::Box<RustAsyncDnsResolver> source;
};
struct ResultSource {
  explicit ResultSource(rust::Box<RustAsyncDnsResult> source) : source(std::move(source)) {}
  rust::Box<RustAsyncDnsResult> source;
};
class Result final : public webrtc::AsyncDnsResolverResult {
 public:
  explicit Result(rust::Box<RustAsyncDnsResult> source) : source_(std::make_shared<ResultSource>(std::move(source))) {}
  std::size_t identity() const { return async_dns_result_identity(*source_->source); }
  bool GetResolvedAddress(int family, webrtc::SocketAddress* address) const override {
    auto source = source_;
    auto resolved = async_dns_result_address(*source->source, family);
    if (!resolved) return false;
    *address = resolved->value;
    return true;
  }
  int GetError() const override {
    auto source = source_;
    return async_dns_result_error(*source->source);
  }
 private:
  std::shared_ptr<ResultSource> source_;
};
struct ResultViews { std::vector<std::unique_ptr<Result>> values; };
class Resolver final : public webrtc::AsyncDnsResolverInterface {
 public:
  Resolver(rust::Box<RustAsyncDnsResolver> source, std::shared_ptr<ResolverScope> scope)
      : source_(std::make_shared<ResolverSource>(std::move(source))), scope_(std::move(scope)), results_(std::make_shared<ResultViews>()) {}
  ~Resolver() override { scope_->Retire(); }
  void Start(const webrtc::SocketAddress& address, Callback callback) override {
    StartImpl(address, false, 0, std::move(callback));
  }
  void Start(const webrtc::SocketAddress& address, int family, Callback callback) override {
    StartImpl(address, true, family, std::move(callback));
  }
  const webrtc::AsyncDnsResolverResult& result() const override {
    auto source = source_;
    auto results = results_;
    auto view = async_dns_result_view(*source->source);
    const auto identity = async_dns_result_identity(*view);
    for (const auto& existing : results->values) {
      if (existing->identity() == identity) return *existing;
    }
    results->values.push_back(std::make_unique<Result>(std::move(view)));
    return *results->values.back();
  }
 private:
  void StartImpl(const webrtc::SocketAddress& address, bool has_family, int family, Callback callback) {
    // Old callback disposal can reenter product ownership. Retain both sources
    // before Begin, and never read product members after calling into Rust.
    auto source = source_;
    auto scope = scope_;
    auto input = std::make_unique<NativeSocketAddress>(address);
    auto request = scope->Begin(std::move(callback));
    async_dns_start(*source->source, std::move(input), has_family, family,
        std::make_unique<NativeDnsResolutionCompletion>(std::move(request)));
  }
  std::shared_ptr<ResolverSource> source_;
  std::shared_ptr<ResolverScope> scope_;
  std::shared_ptr<ResultViews> results_;
};
struct FactorySource {
  explicit FactorySource(rust::Box<RustAsyncDnsFactory> source) : source(std::move(source)) {}
  rust::Box<RustAsyncDnsFactory> source;
};
class Factory final : public webrtc::AsyncDnsResolverFactoryInterface {
 public:
  explicit Factory(rust::Box<RustAsyncDnsFactory> source) : source_(std::make_shared<FactorySource>(std::move(source))) {}
  std::unique_ptr<webrtc::AsyncDnsResolverInterface> Create() override {
    auto source = source_;
    return std::make_unique<Resolver>(async_dns_factory_create(*source->source), std::make_shared<ResolverScope>());
  }
  std::unique_ptr<webrtc::AsyncDnsResolverInterface> CreateAndResolve(const webrtc::SocketAddress& address, Callback callback) override {
    return CreateStarted(address, false, 0, std::move(callback));
  }
  std::unique_ptr<webrtc::AsyncDnsResolverInterface> CreateAndResolve(const webrtc::SocketAddress& address, int family, Callback callback) override {
    return CreateStarted(address, true, family, std::move(callback));
  }
 private:
  std::unique_ptr<webrtc::AsyncDnsResolverInterface> CreateStarted(const webrtc::SocketAddress& address, bool has_family, int family, Callback callback) {
    auto scope = std::make_shared<ResolverScope>();
    auto request = scope->Begin(std::move(callback));
    auto factory = source_;
    auto source = async_dns_factory_create_and_resolve(*factory->source, std::make_unique<NativeSocketAddress>(address), has_family, family,
        std::make_unique<NativeDnsResolutionCompletion>(std::move(request)));
    return std::make_unique<Resolver>(std::move(source), std::move(scope));
  }
  std::shared_ptr<FactorySource> source_;
};
}
NativeDnsResolutionCompletion::NativeDnsResolutionCompletion(std::shared_ptr<Request> request) : request(std::move(request)) {}
NativeDnsResolutionCompletion::~NativeDnsResolutionCompletion() { request->Discard(); }
std::uint8_t dns_resolution_complete(const NativeDnsResolutionCompletion& completion) noexcept { return completion.request->Complete(); }
std::unique_ptr<webrtc::AsyncDnsResolverFactoryInterface> async_dns_factory_adapter(rust::Box<RustAsyncDnsFactory> source) {
  if (!async_dns_factory_present(*source)) return nullptr;
  return std::make_unique<Factory>(std::move(source));
}
}  // namespace pulsebeam::webrtc_sys
