//! 无状态会话 token：HMAC-SHA256 签名的自包含 token。
//! 复用已有的 `sha2` 手写 HMAC，零新依赖；key 每进程随机，重启即全局失效。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// token 载荷（自包含过期时间）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 过期时间（unix 秒）
    pub exp: u64,
    /// 签发时间（unix 秒）
    pub iat: u64,
}

/// 生成每进程随机 32 字节 key（两个 uuid v4 拼接，约 244 bit 熵）
pub fn random_key() -> [u8; 32] {
    let a = *uuid::Uuid::new_v4().as_bytes();
    let b = *uuid::Uuid::new_v4().as_bytes();
    let mut key = [0u8; 32];
    key[..16].copy_from_slice(&a);
    key[16..].copy_from_slice(&b);
    key
}

/// 签发 token：`b64url(payload).b64url(hmac)`
pub fn mint(key: &[u8; 32], ttl: Duration, now: u64) -> String {
    use base64::Engine;
    let claims = Claims {
        exp: now.saturating_add(ttl.as_secs()),
        iat: now,
    };
    let payload = serde_json::to_vec(&claims).expect("claims serialize");
    let sig = hmac_sha256(key, &payload);
    let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    format!("{}.{}", enc.encode(&payload), enc.encode(sig))
}

/// 校验 token：重算 HMAC → 常数时间比较 → 检查未过期。无效返回 None。
pub fn verify(key: &[u8; 32], token: &str, now: u64) -> Option<Claims> {
    use base64::Engine;
    let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;

    let (p_b64, s_b64) = token.split_once('.')?;
    let payload = enc.decode(p_b64).ok()?;
    let sig = enc.decode(s_b64).ok()?;

    let expected = hmac_sha256(key, &payload);
    if !ct_eq(&sig, &expected) {
        return None;
    }

    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    if claims.exp <= now {
        return None;
    }
    Some(claims)
}

/// 手写 HMAC-SHA256（标准 ipad/opad），复用 `sha2`，避免引入 `hmac` crate
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    const BLOCK: usize = 64;

    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        k[..32].copy_from_slice(&digest);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);

    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

/// 常数时间比较，避免计时侧信道（用于 token 签名与密码比对）
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_then_verify_roundtrip() {
        let key = random_key();
        let tok = mint(&key, Duration::from_secs(3600), 1000);
        let claims = verify(&key, &tok, 1000).expect("valid");
        assert_eq!(claims.iat, 1000);
        assert_eq!(claims.exp, 4600);
    }

    #[test]
    fn expired_token_rejected() {
        let key = random_key();
        let tok = mint(&key, Duration::from_secs(100), 1000);
        // now 超过 exp
        assert!(verify(&key, &tok, 2000).is_none());
    }

    #[test]
    fn tampered_signature_rejected() {
        let key = random_key();
        let tok = mint(&key, Duration::from_secs(3600), 1000);
        let mut bad = tok.clone();
        bad.pop();
        bad.push(if tok.ends_with('A') { 'B' } else { 'A' });
        assert!(verify(&key, &bad, 1000).is_none());
    }

    #[test]
    fn wrong_key_rejected() {
        let k1 = random_key();
        let k2 = random_key();
        let tok = mint(&k1, Duration::from_secs(3600), 1000);
        assert!(verify(&k2, &tok, 1000).is_none());
    }

    #[test]
    fn malformed_token_rejected() {
        let key = random_key();
        assert!(verify(&key, "not-a-token", 1000).is_none());
        assert!(verify(&key, "only.one", 1000).is_none());
    }

    #[test]
    fn hmac_matches_known_vector() {
        // RFC 4231 Test Case 2: key="Jefe", data="what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}
