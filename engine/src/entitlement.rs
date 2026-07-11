/// Entitlement signature verification.
///
/// The server signs a **canonical subset** of the entitlement document:
///   { device_id, features, issued_at, last_refresh_time, plan, status, user_id, valid_until }
/// serialised as compact JSON with keys in lexicographic order (BTreeMap).
///
/// `last_refresh_time` MUST be signed: it is what gates whether the CLI
/// re-contacts the server to pick up revocation. If it were left unsigned,
/// a user could edit entitlement.json locally to set `last_refresh_time` to
/// a far-future timestamp, permanently defeating `should_refresh_entitlement`
/// and never re-checking with the server again — the client would keep
/// treating an already-revoked (server-side) entitlement as valid for the
/// entire remainder of its original `valid_until` + grace window, which can
/// be arbitrarily long depending on plan length.
///
/// `device_id` MUST be signed. Otherwise a copied entitlement can replace the
/// stored device id with one derived on another machine while retaining a
/// valid server signature.
///
/// `payment_pending` and `next_billing_at` are intentionally excluded —
/// they are display-only hints and never participate in `is_pro_allowed()`,
/// so tampering with them cannot grant unauthorized access.
///
/// Key rotation: add the new public key to ENTITLEMENT_PUBLIC_KEYS.
/// Keep the old key for the grace period, then remove it.
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, VerifyingKey, Verifier};
use std::collections::BTreeMap;

/// Embedded public keys (base64-encoded raw 32-byte Ed25519 public keys).
/// Key rotation: add the new public key here, then remove the old one once
/// all issued entitlements have been re-signed by the server.
const ENTITLEMENT_PUBLIC_KEYS: &[&str] = &[
    // Rotated 2026-07-11: previous key was exposed in repository history.
    "yq3GKlXgzYN9UIXexDwYzS6oWITssjONUn1HfWuqhRs=",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureVerifyResult {
    Valid,
    /// JSON could not be parsed or signature bytes are malformed.
    Malformed,
    /// Well-formed but does not match any known public key.
    Invalid,
}

/// Extract the canonical signing payload from raw entitlement JSON.
/// Returns `None` if the JSON cannot be parsed or is missing required fields.
pub fn extract_canonical_signing_payload(raw_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let obj = value.as_object()?;

    let mut payload: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for key in [
        "device_id",
        "features",
        "issued_at",
        "last_refresh_time",
        "plan",
        "status",
        "user_id",
        "valid_until",
    ] {
        payload.insert(key, obj.get(key)?.clone());
    }

    serde_json::to_string(&payload).ok()
}

/// Verify that `signature_b64` is a valid Ed25519 signature over the
/// canonical signing payload derived from `raw_entitlement_json`.
pub fn verify_entitlement_signature(
    raw_entitlement_json: &str,
    signature_b64: &str,
) -> SignatureVerifyResult {
    let canonical = match extract_canonical_signing_payload(raw_entitlement_json) {
        Some(c) => c,
        None => return SignatureVerifyResult::Malformed,
    };

    let sig_bytes = match B64.decode(signature_b64.trim()) {
        Ok(bytes) => bytes,
        Err(_) => return SignatureVerifyResult::Malformed,
    };
    let sig_array: [u8; 64] = match sig_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return SignatureVerifyResult::Malformed,
    };
    let signature = Signature::from_bytes(&sig_array);

    #[cfg(test)]
    let test_keys = [TEST_ENTITLEMENT_PUBLIC_KEY];
    #[cfg(not(test))]
    let test_keys: [&str; 0] = [];
    let all_keys = ENTITLEMENT_PUBLIC_KEYS.iter().chain(test_keys.iter()).copied();

    for pub_key_b64 in all_keys {
        let pub_key_bytes = match B64.decode(pub_key_b64) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let pub_key_array: [u8; 32] = match pub_key_bytes.try_into() {
            Ok(arr) => arr,
            Err(_) => continue,
        };
        let verifying_key = match VerifyingKey::from_bytes(&pub_key_array) {
            Ok(k) => k,
            Err(_) => continue,
        };
        if verifying_key.verify(canonical.as_bytes(), &signature).is_ok() {
            return SignatureVerifyResult::Valid;
        }
    }

    SignatureVerifyResult::Invalid
}

/// Convenience wrapper: true only when Valid.
pub fn entitlement_signature_is_valid(raw_entitlement_json: &str, signature_b64: &str) -> bool {
    verify_entitlement_signature(raw_entitlement_json, signature_b64)
        == SignatureVerifyResult::Valid
}

// ─── Test-only helpers ───────────────────────────────────────────────────────

/// Sign `raw_entitlement_json` using the test private key (corresponds to the
/// embedded public key).  Only available in test builds.
///
/// Writes `(entitlement.json, entitlement.sig)` as a matched pair so that
/// `load_entitlement_status_from_dir` will accept them.
#[cfg(test)]
pub fn sign_for_test(raw_entitlement_json: &str) -> String {
    let canonical = extract_canonical_signing_payload(raw_entitlement_json)
        .expect("sign_for_test: JSON must contain required entitlement fields");

    sign_canonical_for_test(&canonical)
}

/// Signs `canonical` with the test-only Ed25519 key.
/// This key pair is used exclusively in tests and does NOT match any key in
/// `ENTITLEMENT_PUBLIC_KEYS`, so signatures produced here are rejected by the
/// production verifier.
#[cfg(test)]
fn sign_canonical_for_test(canonical: &str) -> String {
    use ed25519_dalek::{Signer, SigningKey};
    // Test-only private key. The corresponding public key is TEST_ENTITLEMENT_PUBLIC_KEY
    // below, which is intentionally absent from ENTITLEMENT_PUBLIC_KEYS.
    let priv_bytes = B64
        .decode("AHYXomTTrs2Zc1nbsTfZsWPdE1msrwPB1MzGAlXbpLg=")
        .expect("hardcoded test private key is valid base64");
    let priv_array: [u8; 32] = priv_bytes.try_into().expect("private key is 32 bytes");
    let signing_key = SigningKey::from_bytes(&priv_array);
    let sig = signing_key.sign(canonical.as_bytes());
    B64.encode(sig.to_bytes())
}

/// The public key matching `sign_canonical_for_test`. Added to
/// `ENTITLEMENT_PUBLIC_KEYS` only inside tests that need to accept test-signed
/// entitlements.
#[cfg(test)]
const TEST_ENTITLEMENT_PUBLIC_KEY: &str = "g5DqL4xbgVzepeDUHWNkhPCEU9UUJo+ElApyCyyu/ro=";

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_legacy_payload_without_device_id(raw_entitlement_json: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(raw_entitlement_json).unwrap();
        let obj = value.as_object().unwrap();
        let mut payload: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
        for key in [
            "features",
            "issued_at",
            "last_refresh_time",
            "plan",
            "status",
            "user_id",
            "valid_until",
        ] {
            payload.insert(key, obj.get(key).unwrap().clone());
        }
        let canonical = serde_json::to_string(&payload).unwrap();
        sign_canonical_for_test(&canonical)
    }

    fn make_entitlement(plan: &str, status: &str, features: &[&str]) -> String {
        serde_json::json!({
            "user_id": "u1",
            "plan": plan,
            "features": features,
            "issued_at": "2026-01-01T00:00:00Z",
            "valid_until": "2999-01-01T00:00:00Z",
            "status": status,
            "device_id": "some-device-id",
            "last_refresh_time": "2026-01-01T00:00:00Z",
            "payment_pending": false,
            "next_billing_at": null,
        }).to_string()
    }

    #[test]
    fn malformed_base64_signature_is_rejected() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        assert_eq!(
            verify_entitlement_signature(&json, "!!!not-base64!!!"),
            SignatureVerifyResult::Malformed
        );
    }

    #[test]
    fn short_signature_after_decode_is_malformed() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let short = B64.encode([0u8; 32]);
        assert_eq!(
            verify_entitlement_signature(&json, &short),
            SignatureVerifyResult::Malformed
        );
    }

    #[test]
    fn invalid_json_entitlement_is_malformed() {
        assert_eq!(
            verify_entitlement_signature("not-json", "AAAA"),
            SignatureVerifyResult::Malformed
        );
    }

    #[test]
    fn signature_from_unknown_key_is_invalid() {
        // Sign with a random key NOT in ENTITLEMENT_PUBLIC_KEYS
        use ed25519_dalek::{Signer, SigningKey};
        let other_key = SigningKey::from_bytes(&[99u8; 32]);
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let canonical = extract_canonical_signing_payload(&json).unwrap();
        let sig = B64.encode(other_key.sign(canonical.as_bytes()).to_bytes());
        assert_eq!(
            verify_entitlement_signature(&json, &sig),
            SignatureVerifyResult::Invalid
        );
    }

    #[test]
    fn tampered_plan_makes_signature_invalid() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let sig = sign_for_test(&json);
        // Change plan to core_free — canonical payload differs
        let tampered = json.replace("pro_personal_monthly", "core_free");
        assert_ne!(
            verify_entitlement_signature(&tampered, &sig),
            SignatureVerifyResult::Valid
        );
    }

    #[test]
    fn tampered_device_id_invalidates_signature() {
        let json1 = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let sig = sign_for_test(&json1);
        let json2 = json1.replace("some-device-id", "completely-different-device");
        assert_ne!(
            verify_entitlement_signature(&json2, &sig),
            SignatureVerifyResult::Valid,
            "device_id must be covered by the entitlement signature"
        );
    }

    #[test]
    fn canonical_payload_matches_server_byte_contract() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        assert_eq!(
            extract_canonical_signing_payload(&json).as_deref(),
            Some(
                r#"{"device_id":"some-device-id","features":["observe"],"issued_at":"2026-01-01T00:00:00Z","last_refresh_time":"2026-01-01T00:00:00Z","plan":"pro_personal_monthly","status":"active","user_id":"u1","valid_until":"2999-01-01T00:00:00Z"}"#
            )
        );
    }

    #[test]
    fn legacy_signature_without_device_binding_is_rejected() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let legacy_sig = sign_legacy_payload_without_device_id(&json);

        assert_ne!(
            verify_entitlement_signature(&json, &legacy_sig),
            SignatureVerifyResult::Valid,
            "legacy entitlements must be re-issued with a device-bound signature"
        );
    }

    #[test]
    fn tampered_last_refresh_time_invalidates_signature() {
        // Regression test for the revocation-bypass bug: last_refresh_time
        // gates whether the client ever re-contacts the server to pick up a
        // revocation. If it were excluded from the signed payload, editing
        // it locally to a far-future date would silently and permanently
        // defeat should_refresh_entitlement(), letting an already-revoked
        // entitlement keep passing signature checks until its original
        // valid_until + grace window naturally elapsed.
        let json = make_entitlement("pro_personal_monthly", "active", &["observe"]);
        let sig = sign_for_test(&json);

        // Tamper with ONLY last_refresh_time via structured JSON edit —
        // issued_at shares the same literal value in the fixture, so a naive
        // string replace would touch both fields and understate what this
        // test is isolating.
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value["last_refresh_time"] = serde_json::Value::String("2999-01-01T00:00:00Z".to_string());
        let tampered = value.to_string();

        assert_ne!(
            value["issued_at"], value["last_refresh_time"],
            "sanity check: test must isolate last_refresh_time from issued_at"
        );
        assert_ne!(
            verify_entitlement_signature(&tampered, &sig),
            SignatureVerifyResult::Valid,
            "tampering with last_refresh_time must invalidate the signature"
        );
    }

    #[test]
    fn round_trip_sign_and_verify() {
        let json = make_entitlement("pro_personal_monthly", "active", &["observe", "notify"]);
        let sig = sign_for_test(&json);
        assert_eq!(
            verify_entitlement_signature(&json, &sig),
            SignatureVerifyResult::Valid
        );
    }

    #[test]
    fn convenience_wrapper_returns_false_for_invalid() {
        assert!(!entitlement_signature_is_valid("not-json", "AAAA"));
    }

}
