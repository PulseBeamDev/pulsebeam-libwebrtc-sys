#pragma once
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
#include "api/async_dns_resolver.h"
namespace pulsebeam::webrtc_sys {
struct RustAsyncDnsFactory;
class NativeDnsResolutionCompletion {
 public:
  struct Request;
  explicit NativeDnsResolutionCompletion(std::shared_ptr<Request> request);
  ~NativeDnsResolutionCompletion();
  std::shared_ptr<Request> request;
};
std::uint8_t dns_resolution_complete(const NativeDnsResolutionCompletion& completion) noexcept;
std::unique_ptr<webrtc::AsyncDnsResolverFactoryInterface> async_dns_factory_adapter(rust::Box<RustAsyncDnsFactory> source);
}  // namespace pulsebeam::webrtc_sys
