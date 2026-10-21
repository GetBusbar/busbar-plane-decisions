//! The transport this plane's claims are made over.
//!
//! A claim is the only way a plane names a transport, and it names it as a claim, never as a
//! connection. Each dialect has one exact-path claim ([`crate::driven::tail::CLAIMS`]), spoken over
//! HTTP, carrying a bearer credential this plane never sees: the kernel verifies the caller, and
//! presents the member's own credential to the far end.

/// The request transport every claim of this plane is made over, and the one its outbound need
/// names.
pub const TRANSPORT: &str = "http";
