use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPath {
    Zen4Avx512,
    GenericAvx2,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CpuProfile {
    pub vendor: String,
    pub has_avx2: bool,
    pub has_avx512f: bool,
    pub has_avx512vl: bool,
    pub has_fma: bool,
    pub has_bmi2: bool,
    pub has_sha: bool,
    pub path: ExecutionPath,
}

impl CpuProfile {
    pub fn detect() -> Result<Self> {
        #[cfg(not(target_arch = "x86_64"))]
        {
            bail!("Cabal MCP Runtime требует x86_64; текущая архитектура не поддерживается");
        }

        #[cfg(target_arch = "x86_64")]
        {
            let vendor = detect_vendor()?;
            let has_avx2 = std::is_x86_feature_detected!("avx2");
            if !has_avx2 {
                bail!("CPU без AVX2 не поддерживается: fallback ниже AVX2 запрещён");
            }
            let has_avx512f = std::is_x86_feature_detected!("avx512f");
            let has_avx512vl = std::is_x86_feature_detected!("avx512vl");
            let has_fma = std::is_x86_feature_detected!("fma");
            let has_bmi2 = std::is_x86_feature_detected!("bmi2");
            let has_sha = std::is_x86_feature_detected!("sha");

            let is_zen4_profile = vendor == "AuthenticAMD"
                && has_avx512f
                && has_avx512vl
                && has_fma
                && has_bmi2
                && has_sha;

            let path = if is_zen4_profile {
                ExecutionPath::Zen4Avx512
            } else {
                ExecutionPath::GenericAvx2
            };

            Ok(Self {
                vendor,
                has_avx2,
                has_avx512f,
                has_avx512vl,
                has_fma,
                has_bmi2,
                has_sha,
                path,
            })
        }
    }

    pub fn hash_bytes(&self, input: &[u8]) -> u64 {
        #[cfg(target_arch = "x86_64")]
        {
            match self.path {
                ExecutionPath::Zen4Avx512 => {
                    // SAFETY: path выбирается только после runtime-проверки feature set.
                    unsafe { hash_avx512(input) }
                }
                ExecutionPath::GenericAvx2 => {
                    // SAFETY: запуск разрешён только при наличии AVX2.
                    unsafe { hash_avx2(input) }
                }
            }
        }

        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = input;
            0
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn detect_vendor() -> Result<String> {
    use std::arch::x86_64::__cpuid;

    let leaf0 = __cpuid(0);
    let mut bytes = [0u8; 12];
    bytes[0..4].copy_from_slice(&leaf0.ebx.to_le_bytes());
    bytes[4..8].copy_from_slice(&leaf0.edx.to_le_bytes());
    bytes[8..12].copy_from_slice(&leaf0.ecx.to_le_bytes());
    let vendor = String::from_utf8(bytes.to_vec()).context("cpuid vendor decode failed")?;
    Ok(vendor)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512vl,avx2,fma,bmi2,sha")]
unsafe fn hash_avx512(input: &[u8]) -> u64 {
    use std::arch::x86_64::{
        __m512i, _mm512_loadu_si512, _mm512_setzero_si512, _mm512_storeu_si512, _mm512_xor_si512,
    };

    let mut acc: __m512i = _mm512_setzero_si512();
    let mut i = 0usize;
    while i + 64 <= input.len() {
        // SAFETY: границы проверены условием цикла.
        let chunk = unsafe { _mm512_loadu_si512(input.as_ptr().add(i) as *const _) };
        acc = _mm512_xor_si512(acc, chunk);
        i += 64;
    }

    let mut lanes = [0u64; 8];
    // SAFETY: lanes достаточного размера.
    unsafe { _mm512_storeu_si512(lanes.as_mut_ptr() as *mut _, acc) };

    let mut h = 0xcbf29ce484222325u64;
    for v in lanes {
        h ^= v;
        h = h.rotate_left(13).wrapping_mul(0x9e3779b185ebca87);
    }
    for &b in &input[i..] {
        h ^= b as u64;
        h = h.rotate_left(5).wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hash_avx2(input: &[u8]) -> u64 {
    use std::arch::x86_64::{
        __m256i, _mm256_loadu_si256, _mm256_setzero_si256, _mm256_storeu_si256, _mm256_xor_si256,
    };

    let mut acc: __m256i = _mm256_setzero_si256();
    let mut i = 0usize;
    while i + 32 <= input.len() {
        // SAFETY: границы проверены условием цикла.
        let chunk = unsafe { _mm256_loadu_si256(input.as_ptr().add(i) as *const _) };
        acc = _mm256_xor_si256(acc, chunk);
        i += 32;
    }

    let mut lanes = [0u64; 4];
    // SAFETY: lanes достаточного размера.
    unsafe { _mm256_storeu_si256(lanes.as_mut_ptr() as *mut _, acc) };

    let mut h = 0xcbf29ce484222325u64;
    for v in lanes {
        h ^= v;
        h = h.rotate_left(11).wrapping_mul(0x9e3779b185ebca87);
    }
    for &b in &input[i..] {
        h ^= b as u64;
        h = h.rotate_left(5).wrapping_mul(0x100000001b3);
    }
    h
}
