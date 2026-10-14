#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include "rust/cxx.h"
#include "rtc_base/ssl_certificate.h"

namespace pulsebeam::webrtc_sys {
struct RustTlsCertificateVerifier;
struct FfiTlsCertificateStats;
class NativeTlsCertificate final {
 public:
  explicit NativeTlsCertificate(std::unique_ptr<webrtc::SSLCertificate> certificate) noexcept;
  const webrtc::SSLCertificate& certificate() const noexcept;
  std::unique_ptr<webrtc::SSLCertificate> Take() noexcept;
 private:
  std::unique_ptr<webrtc::SSLCertificate> certificate_;
};
class NativeTlsCertificateChain final {
 public:
  explicit NativeTlsCertificateChain(std::unique_ptr<webrtc::SSLCertChain> chain) noexcept;
  const webrtc::SSLCertChain& chain() const noexcept;
  std::unique_ptr<webrtc::SSLCertChain> Take() noexcept;
  void Append(std::unique_ptr<NativeTlsCertificate> certificate) noexcept;
 private:
  std::unique_ptr<webrtc::SSLCertChain> chain_;
};
std::unique_ptr<NativeTlsCertificate> tls_certificate_from_pem(rust::Str pem) noexcept;
std::unique_ptr<NativeTlsCertificate> tls_certificate_clone(const NativeTlsCertificate& certificate) noexcept;
rust::String tls_certificate_pem(const NativeTlsCertificate& certificate) noexcept;
rust::Vec<std::uint8_t> tls_certificate_der(const NativeTlsCertificate& certificate) noexcept;
rust::String tls_certificate_signature_digest(const NativeTlsCertificate& certificate, bool& valid) noexcept;
rust::Vec<std::uint8_t> tls_certificate_digest(const NativeTlsCertificate& certificate, rust::Str algorithm, std::size_t capacity, bool& valid) noexcept;
std::int64_t tls_certificate_expiration(const NativeTlsCertificate& certificate) noexcept;
rust::Vec<FfiTlsCertificateStats> tls_certificate_stats(const NativeTlsCertificate& certificate) noexcept;
std::unique_ptr<NativeTlsCertificateChain> tls_chain_new() noexcept;
void tls_chain_append(NativeTlsCertificateChain& chain, std::unique_ptr<NativeTlsCertificate> certificate) noexcept;
std::unique_ptr<NativeTlsCertificateChain> tls_chain_clone(const NativeTlsCertificateChain& chain) noexcept;
std::size_t tls_chain_size(const NativeTlsCertificateChain& chain) noexcept;
std::unique_ptr<NativeTlsCertificate> tls_chain_get(const NativeTlsCertificateChain& chain, std::size_t index) noexcept;
rust::Vec<FfiTlsCertificateStats> tls_chain_stats(const NativeTlsCertificateChain& chain) noexcept;
std::unique_ptr<webrtc::SSLCertificateVerifier> tls_certificate_verifier_adapter(rust::Box<RustTlsCertificateVerifier> verifier) noexcept;
}
