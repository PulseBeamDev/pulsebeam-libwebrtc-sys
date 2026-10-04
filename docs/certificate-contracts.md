# Native certificate resource contracts

`Certificate` owns a retained native `RTCCertificate`, not a borrowed peer or SSL backend pointer. Cloning retains the same resource; importing exported PEM creates a distinct native resource. `same_resource` exposes scoped-reference identity, which native RTCConfiguration certificate comparisons use. Rust value equality instead invokes native RTCCertificate equality.

## Native generation and queries

`CertificateKeyParams` exposes RSA modulus/exponent parameters and the pinned generator's meaningful ECDSA curve, P-256. Defaults and validity come from the selected native binary. RSA exponent values preserve native's unsigned parameter domain; the binding does not introduce an exponent allowlist. Native parameter validity does not guarantee successful backend generation.

`Certificate::generate` calls native synchronous generation and can be expensive. It uses native cryptographic randomness and system epoch time, not the environment's manual clock. Its optional duration follows native lifetime semantics: milliseconds are truncated to seconds, native caps the lifetime at one year, and absence selects the native 30-day default. Durations larger than the native unsigned millisecond domain saturate before the same native cap. Invalid key parameters return InvalidParameter; a null native generation result returns Internal. This method does not provide an asynchronous generator or claim controlled-world deterministic generation.

`from_pem` delegates to RTCCertificate::FromPEM. `from_pem_chain` delegates to native SSLIdentity chain import and retains the complete chain. These are native parsing operations, not additional binding policies for certificate trust or expiration. `to_pem` exports private-key material and the leaf certificate, including for a multi-member chain, matching native ToPEM. Exported strings are ordinary owned strings and are not guaranteed to be zeroized; Debug omits the private key.

Expiration queries preserve native unsigned Unix epoch milliseconds and native HasExpired semantics. Chain snapshots contain copied PEM, DER, signed Unix expiration seconds, and optional native signature digest names. Only the documented native expiration-unavailable sentinel (-1) becomes None. Fingerprint lookup delegates to SSLFingerprint::Create and preserves native algorithm support and RFC 4572 formatting without a binding allowlist.

## Ownership and cross-thread safety

The safe resource is Send + Sync for the selected pinned BoringSSL artifacts:

- RTCCertificate uses atomic RefCountedNonVirtual ownership and owns its identity.
- The pinned BoringSSL certificate implementation stores immutable CRYPTO_BUFFER bytes. BoringSSL's pool.h permits simultaneous reads and reference-count operations.
- OpenSSLKeyPair owns an EVP_PKEY. BoringSSL's evp.h permits concurrent non-mutating key operations, including serialization and signing, but not concurrent mutation.
- Native identity clones retain these backend resources and install them into distinct SSL contexts. The wrapper exposes no identity, key, context, or certificate mutation.
- Final destruction releases retained backend references, not pointers borrowed from a peer.

The exported SDK now includes the pinned BoringSSL headers. Adapter compilation is guarded by OPENSSL_IS_BORINGSSL so it cannot silently select sysroot OpenSSL headers instead. This qualification is not a claim about arbitrary OpenSSL backends.

Source authority: pinned rtc_base/rtc_certificate*, rtc_base/rtc_certificate_generator*, rtc_base/ssl_identity*, rtc_base/boringssl_certificate*, rtc_base/boringssl_identity*, rtc_base/openssl_key_pair*, and third_party/boringssl/src/include/openssl/{pool,evp}.h.

The resource tests cover native defaults/validity, lifetime capping, malformed PEM failure, chain retention, fingerprints, expiration boundaries, identity versus value equality, concurrent immutable queries, and final release on a caller thread. Peer configuration tests additionally cover retained certificate identity and actor migration followed by native SDP fingerprint generation; the controlled two-peer test checks both supplied fingerprints and completes DTLS/SCTP application-byte transport. See [peer configuration contracts](peer-configuration-contracts.md). Asynchronous generation remains separate provider work.
