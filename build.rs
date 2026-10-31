use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const FIELD_SIZE: usize = 256;

const GENERATING_POLYNOMIAL: usize = 29;

fn gen_log_table(polynomial: usize) -> [u8; FIELD_SIZE] {
    let mut result: [u8; FIELD_SIZE] = [0; FIELD_SIZE];
    let mut b: usize = 1;

    for log in 0..FIELD_SIZE - 1 {
        result[b] = log as u8;

        b <<= 1;

        if FIELD_SIZE <= b {
            b = (b - FIELD_SIZE) ^ polynomial;
        }
    }

    result
}

const EXP_TABLE_SIZE: usize = FIELD_SIZE * 2 - 2;

fn gen_exp_table(log_table: &[u8; FIELD_SIZE]) -> [u8; EXP_TABLE_SIZE] {
    let mut result: [u8; EXP_TABLE_SIZE] = [0; EXP_TABLE_SIZE];

    for (i, &log_entry) in log_table.iter().enumerate().skip(1) {
        let log = log_entry as usize;
        result[log] = i as u8;
        result[log + FIELD_SIZE - 1] = i as u8;
    }

    result
}

fn multiply(log_table: &[u8; FIELD_SIZE], exp_table: &[u8; EXP_TABLE_SIZE], a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        0
    } else {
        let log_a = log_table[a as usize];
        let log_b = log_table[b as usize];
        let log_result = log_a as usize + log_b as usize;
        exp_table[log_result]
    }
}

fn gen_mul_table(
    log_table: &[u8; FIELD_SIZE],
    exp_table: &[u8; EXP_TABLE_SIZE],
) -> [[u8; FIELD_SIZE]; FIELD_SIZE] {
    let mut result: [[u8; FIELD_SIZE]; FIELD_SIZE] = [[0; 256]; 256];

    for (a, row) in result.iter_mut().enumerate() {
        for (b, cell) in row.iter_mut().enumerate() {
            *cell = multiply(log_table, exp_table, a as u8, b as u8);
        }
    }

    result
}

fn gen_mul_table_half(
    log_table: &[u8; FIELD_SIZE],
    exp_table: &[u8; EXP_TABLE_SIZE],
) -> ([[u8; 16]; FIELD_SIZE], [[u8; 16]; FIELD_SIZE]) {
    let mut low: [[u8; 16]; FIELD_SIZE] = [[0; 16]; FIELD_SIZE];
    let mut high: [[u8; 16]; FIELD_SIZE] = [[0; 16]; FIELD_SIZE];

    for a in 0..low.len() {
        for b in 0..low.len() {
            let mut result = 0;
            if !(a == 0 || b == 0) {
                let log_a = log_table[a];
                let log_b = log_table[b];
                result = exp_table[log_a as usize + log_b as usize];
            }
            if (b & 0x0F) == b {
                low[a][b] = result;
            }
            if (b & 0xF0) == b {
                high[a][b >> 4] = result;
            }
        }
    }
    (low, high)
}

macro_rules! write_table {
    (1D => $file:ident, $table:ident, $name:expr, $type:expr) => {{
        let len = $table.len();
        let mut table_str = String::from(format!("pub static {}: [{}; {}] = [", $name, $type, len));

        for v in $table.iter() {
            let str = format!("{}, ", v);
            table_str.push_str(&str);
        }

        table_str.push_str("];\n");

        $file.write_all(table_str.as_bytes()).unwrap();
    }};
    (2D => $file:ident, $table:ident, $name:expr, $type:expr) => {{
        let rows = $table.len();
        let cols = $table[0].len();
        let mut table_str = String::from(format!(
            "pub static {}: [[{}; {}]; {}] = [",
            $name, $type, cols, rows
        ));

        for a in $table.iter() {
            table_str.push_str("[");
            for b in a.iter() {
                let str = format!("{}, ", b);
                table_str.push_str(&str);
            }
            table_str.push_str("],\n");
        }

        table_str.push_str("];\n");

        $file.write_all(table_str.as_bytes()).unwrap();
    }};
}

fn write_tables() {
    let log_table = gen_log_table(GENERATING_POLYNOMIAL);
    let exp_table = gen_exp_table(&log_table);
    let mul_table = gen_mul_table(&log_table, &exp_table);

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("table.rs");
    let mut f = File::create(&dest_path).unwrap();

    write_table!(1D => f, log_table,      "LOG_TABLE",      "u8");
    write_table!(1D => f, exp_table,      "EXP_TABLE",      "u8");
    write_table!(2D => f, mul_table,      "MUL_TABLE",      "u8");

    if cfg!(any(
        feature = "simd-neon",
        feature = "simd-ssse3",
        feature = "simd-avx2",
        feature = "simd-avx512",
        feature = "simd-gfni",
        feature = "simd-vsx"
    )) {
        let (mul_table_low, mul_table_high) = gen_mul_table_half(&log_table, &exp_table);

        write_table!(2D => f, mul_table_low,  "MUL_TABLE_LOW",  "u8");
        write_table!(2D => f, mul_table_high, "MUL_TABLE_HIGH", "u8");
    }
}

/// Generate specialized encode functions for common (data_shards, parity_shards) configurations.
///
/// These functions unroll the data shard loop and inline the GF multiplication,
/// eliminating per-shard function pointer dispatch and enabling better register allocation.
fn generate_encode_codegen() {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // Only generate for x86_64 (AVX2) and aarch64 (NEON)
    if target_arch != "x86_64" && target_arch != "aarch64" {
        return;
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("codegen_encode.rs");
    let mut f = File::create(&dest_path).unwrap();

    // Common configurations from MinIO, Ceph, HDFS
    let configs: &[(usize, usize)] = &[(10, 4), (12, 4), (8, 3), (8, 4), (6, 3), (4, 2)];

    if target_arch == "x86_64" {
        generate_encode_codegen_avx2(&mut f, configs);
        generate_encode_codegen_avx512(&mut f, &[(10, 4), (12, 4), (4, 2)]);
    } else if target_arch == "aarch64" {
        generate_encode_codegen_neon(&mut f, configs);
    }
}

fn generate_encode_codegen_avx2(f: &mut File, configs: &[(usize, usize)]) {
    use std::io::Write;

    writeln!(
        f,
        "// Auto-generated AVX2 encode functions for common configurations."
    )
    .unwrap();
    writeln!(f, "// Generated by build.rs — do not edit manually.").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "#[cfg(all(").unwrap();
    writeln!(f, "    feature = \"simd-avx2\",").unwrap();
    writeln!(f, "    target_arch = \"x86_64\",").unwrap();
    writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
    writeln!(
        f,
        "    not(any(target_os = \"android\", target_os = \"ios\"))"
    )
    .unwrap();
    writeln!(f, "))]").unwrap();
    writeln!(f, "mod avx2_impl {{").unwrap();
    writeln!(f, "    use core::arch::x86_64::*;").unwrap();
    writeln!(f).unwrap();

    for &(d, p) in configs {
        generate_encode_fn_avx2(f, d, p);
    }

    writeln!(f, "}}").unwrap();
    writeln!(f).unwrap();

    writeln!(f, "#[cfg(all(").unwrap();
    writeln!(f, "    feature = \"simd-avx2\",").unwrap();
    writeln!(f, "    target_arch = \"x86_64\",").unwrap();
    writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
    writeln!(
        f,
        "    not(any(target_os = \"android\", target_os = \"ios\"))"
    )
    .unwrap();
    writeln!(f, "))]").unwrap();
    generate_encode_selector(f, configs, "avx2");
}

fn generate_encode_codegen_avx512(f: &mut File, configs: &[(usize, usize)]) {
    use std::io::Write;

    writeln!(
        f,
        "// Auto-generated AVX-512BW encode functions for common configurations."
    )
    .unwrap();
    writeln!(f, "#[cfg(all(").unwrap();
    writeln!(f, "    feature = \"simd-avx512\",").unwrap();
    writeln!(f, "    target_arch = \"x86_64\",").unwrap();
    writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
    writeln!(
        f,
        "    not(any(target_os = \"android\", target_os = \"ios\"))"
    )
    .unwrap();
    writeln!(f, "))]").unwrap();
    writeln!(f, "mod avx512_impl {{").unwrap();
    writeln!(f, "    use core::arch::x86_64::*;").unwrap();
    writeln!(f).unwrap();

    for &(d, p) in configs {
        generate_encode_fn_avx512(f, d, p);
    }

    writeln!(f, "}}").unwrap();
    writeln!(f).unwrap();

    writeln!(f, "#[cfg(all(").unwrap();
    writeln!(f, "    feature = \"simd-avx512\",").unwrap();
    writeln!(f, "    target_arch = \"x86_64\",").unwrap();
    writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
    writeln!(
        f,
        "    not(any(target_os = \"android\", target_os = \"ios\"))"
    )
    .unwrap();
    writeln!(f, "))]").unwrap();
    generate_encode_selector(f, configs, "avx512");
}

fn generate_encode_selector(f: &mut File, configs: &[(usize, usize)], isa: &str) {
    writeln!(f, "pub(crate) fn select_encode_kernel_{isa}(").unwrap();
    writeln!(f, "    data_shard_count: usize, parity_shard_count: usize,").unwrap();
    writeln!(
        f,
        ") -> Option<crate::galois_8::codegen::SelectedEncodeKernel> {{"
    )
    .unwrap();
    if isa == "avx2" {
        writeln!(f, "    select_encode_kernel_avx2_with_avx2_available(").unwrap();
        writeln!(
            f,
            "        avx2_codegen_available(), data_shard_count, parity_shard_count,"
        )
        .unwrap();
        writeln!(f, "    )").unwrap();
        writeln!(f, "}}").unwrap();
        writeln!(f, "#[cfg(all(").unwrap();
        writeln!(f, "    feature = \"simd-avx2\",").unwrap();
        writeln!(f, "    target_arch = \"x86_64\",").unwrap();
        writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
        writeln!(
            f,
            "    not(any(target_os = \"android\", target_os = \"ios\"))"
        )
        .unwrap();
        writeln!(f, "))]").unwrap();
        writeln!(f, "fn select_encode_kernel_avx2_with_avx2_available(").unwrap();
        writeln!(
            f,
            "    avx2_available: bool, data_shard_count: usize, parity_shard_count: usize,"
        )
        .unwrap();
        writeln!(
            f,
            ") -> Option<crate::galois_8::codegen::SelectedEncodeKernel> {{"
        )
        .unwrap();
        writeln!(f, "    if !avx2_available {{ return None; }}").unwrap();
    } else if isa == "avx512" {
        writeln!(f, "    select_encode_kernel_avx512_with_avx512_available(").unwrap();
        writeln!(
            f,
            "        avx512_codegen_available(), data_shard_count, parity_shard_count,"
        )
        .unwrap();
        writeln!(f, "    )").unwrap();
        writeln!(f, "}}").unwrap();
        writeln!(f, "#[cfg(all(").unwrap();
        writeln!(f, "    feature = \"simd-avx512\",").unwrap();
        writeln!(f, "    target_arch = \"x86_64\",").unwrap();
        writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
        writeln!(
            f,
            "    not(any(target_os = \"android\", target_os = \"ios\"))"
        )
        .unwrap();
        writeln!(f, "))]").unwrap();
        writeln!(f, "fn select_encode_kernel_avx512_with_avx512_available(").unwrap();
        writeln!(
            f,
            "    avx512_available: bool, data_shard_count: usize, parity_shard_count: usize,"
        )
        .unwrap();
        writeln!(
            f,
            ") -> Option<crate::galois_8::codegen::SelectedEncodeKernel> {{"
        )
        .unwrap();
        writeln!(f, "    if !avx512_available {{ return None; }}").unwrap();
    }
    writeln!(f, "    match (data_shard_count, parity_shard_count) {{").unwrap();
    for &(d, p) in configs {
        writeln!(
            f,
            "        ({d}, {p}) => Some(crate::galois_8::codegen::SelectedEncodeKernel::new("
        )
        .unwrap();
        writeln!(
            f,
            "            {d}, {p}, |parity_rows, data, parity, shard_len| {{"
        )
        .unwrap();
        writeln!(f, "                // SAFETY: selection established ISA availability; SelectedEncodeKernel::execute").unwrap();
        writeln!(f, "                // validates every row and shard before invoking this private callback.").unwrap();
        writeln!(f, "                unsafe {{ {isa}_impl::encode_{d}x{p}_{isa}(parity_rows, data, parity, shard_len); }}").unwrap();
        writeln!(f, "            }},").unwrap();
        writeln!(f, "        )),").unwrap();
    }
    writeln!(f, "        _ => None,").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f, "}}").unwrap();
}

fn generate_encode_fn_avx2(f: &mut File, d: usize, p: usize) {
    use std::io::Write;

    writeln!(
        f,
        "    /// Specialized encode for {d} data + {p} parity shards using AVX2."
    )
    .unwrap();
    writeln!(f, "    ///").unwrap();
    writeln!(f, "    /// # Safety").unwrap();
    writeln!(f, "    ///").unwrap();
    writeln!(
        f,
        "    /// Caller must ensure AVX2 is available and all slices have length >= shard_len."
    )
    .unwrap();
    writeln!(f, "    #[target_feature(enable = \"avx2\")]").unwrap();
    writeln!(f, "    pub(super) unsafe fn encode_{d}x{p}_avx2(").unwrap();
    writeln!(f, "        parity_rows: &[&[u8]],").unwrap();
    writeln!(f, "        data: &[&[u8]],").unwrap();
    writeln!(f, "        parity: &mut [&mut [u8]],").unwrap();
    writeln!(f, "        shard_len: usize,").unwrap();
    writeln!(f, "    ) {{").unwrap();
    writeln!(
        f,
        "        // SAFETY: this function's safety contract applies to each generated row kernel."
    )
    .unwrap();
    writeln!(f, "        unsafe {{").unwrap();
    for pi in 0..p {
        writeln!(
            f,
            "            encode_{d}x{p}_parity_{pi}_avx2(parity_rows, data, parity, shard_len);"
        )
        .unwrap();
    }
    writeln!(f, "        }}").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f).unwrap();

    for pi in 0..p {
        generate_encode_parity_fn_avx2(f, d, p, pi);
    }
}

fn generate_encode_parity_fn_avx2(f: &mut File, d: usize, p: usize, pi: usize) {
    use std::io::Write;

    writeln!(f, "    #[target_feature(enable = \"avx2\")]").unwrap();
    writeln!(f, "    unsafe fn encode_{d}x{p}_parity_{pi}_avx2(").unwrap();
    writeln!(f, "        parity_rows: &[&[u8]],").unwrap();
    writeln!(f, "        data: &[&[u8]],").unwrap();
    writeln!(f, "        parity: &mut [&mut [u8]],").unwrap();
    writeln!(f, "        shard_len: usize,").unwrap();
    writeln!(f, "    ) {{").unwrap();
    writeln!(
        f,
        "        // SAFETY: the parent dispatcher checked AVX2 availability and slice lengths."
    )
    .unwrap();
    writeln!(f, "        unsafe {{").unwrap();
    writeln!(
        f,
        "            let nibble_mask: __m256i = _mm256_set1_epi8(0x0f);"
    )
    .unwrap();
    for di in 0..d {
        writeln!(
            f,
            "            let (coef_low_{di}, coef_high_{di}): (__m256i, __m256i) = {{"
        )
        .unwrap();
        writeln!(f, "                let c = parity_rows[{pi}][{di}];").unwrap();
        writeln!(
            f,
            "                let (lh, hh) = super::super::load_table_halves(c);"
        )
        .unwrap();
        writeln!(
            f,
            "                let low128: __m128i = _mm_loadu_si128(lh.as_ptr().cast());"
        )
        .unwrap();
        writeln!(
            f,
            "                let high128: __m128i = _mm_loadu_si128(hh.as_ptr().cast());"
        )
        .unwrap();
        writeln!(
            f,
            "                (_mm256_broadcastsi128_si256(low128), _mm256_broadcastsi128_si256(high128))"
        )
            .unwrap();
        writeln!(f, "            }};").unwrap();
    }
    writeln!(f).unwrap();
    writeln!(f, "            let bytes_done = shard_len & !31usize;").unwrap();
    writeln!(f, "            let mut offset = 0usize;").unwrap();
    writeln!(f, "            while offset < bytes_done {{").unwrap();
    writeln!(
        f,
        "                let data_vec: __m256i = _mm256_loadu_si256(data[0][offset..].as_ptr().cast());"
    )
        .unwrap();
    writeln!(
        f,
        "                let low = _mm256_and_si256(data_vec, nibble_mask);"
    )
    .unwrap();
    writeln!(
        f,
        "                let high = _mm256_and_si256(_mm256_srli_epi64::<4>(data_vec), nibble_mask);"
    )
        .unwrap();
    writeln!(
        f,
        "                let mut acc: __m256i = _mm256_xor_si256("
    )
    .unwrap();
    writeln!(
        f,
        "                    _mm256_shuffle_epi8(coef_low_0, low),"
    )
    .unwrap();
    writeln!(
        f,
        "                    _mm256_shuffle_epi8(coef_high_0, high),"
    )
    .unwrap();
    writeln!(f, "                );").unwrap();
    for di in 1..d {
        writeln!(
            f,
            "                let data_vec: __m256i = _mm256_loadu_si256(data[{di}][offset..].as_ptr().cast());"
        )
            .unwrap();
        writeln!(
            f,
            "                let low = _mm256_and_si256(data_vec, nibble_mask);"
        )
        .unwrap();
        writeln!(
            f,
            "                let high = _mm256_and_si256(_mm256_srli_epi64::<4>(data_vec), nibble_mask);"
        )
            .unwrap();
        writeln!(
            f,
            "                acc = _mm256_xor_si256(acc, _mm256_xor_si256("
        )
        .unwrap();
        writeln!(
            f,
            "                    _mm256_shuffle_epi8(coef_low_{di}, low),"
        )
        .unwrap();
        writeln!(
            f,
            "                    _mm256_shuffle_epi8(coef_high_{di}, high),"
        )
        .unwrap();
        writeln!(f, "                ));").unwrap();
    }
    writeln!(
        f,
        "                _mm256_storeu_si256(parity[{pi}][offset..].as_mut_ptr().cast(), acc);"
    )
    .unwrap();
    writeln!(f, "                offset += 32;").unwrap();
    writeln!(f, "            }}").unwrap();

    writeln!(f).unwrap();
    writeln!(f, "            for i in bytes_done..shard_len {{").unwrap();
    writeln!(f, "                let mut acc: u8 = 0;").unwrap();
    for di in 0..d {
        writeln!(f, "                acc ^= super::super::super::MUL_TABLE[parity_rows[{pi}][{di}] as usize][data[{di}][i] as usize];").unwrap();
    }
    writeln!(f, "                parity[{pi}][i] = acc;").unwrap();
    writeln!(f, "            }}").unwrap();
    writeln!(f, "        }}").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f).unwrap();
}

fn generate_encode_fn_avx512(f: &mut File, d: usize, p: usize) {
    use std::io::Write;

    writeln!(
        f,
        "    /// Specialized encode for {d} data + {p} parity shards using AVX-512BW."
    )
    .unwrap();
    writeln!(f, "    ///").unwrap();
    writeln!(f, "    /// # Safety").unwrap();
    writeln!(f, "    ///").unwrap();
    writeln!(
        f,
        "    /// Caller must ensure AVX512F and AVX512BW are available and all slices have length >= shard_len."
    )
    .unwrap();
    writeln!(f, "    #[target_feature(enable = \"avx512f,avx512bw\")]").unwrap();
    writeln!(f, "    pub(super) unsafe fn encode_{d}x{p}_avx512(").unwrap();
    writeln!(f, "        parity_rows: &[&[u8]],").unwrap();
    writeln!(f, "        data: &[&[u8]],").unwrap();
    writeln!(f, "        parity: &mut [&mut [u8]],").unwrap();
    writeln!(f, "        shard_len: usize,").unwrap();
    writeln!(f, "    ) {{").unwrap();
    writeln!(
        f,
        "        // SAFETY: this function's safety contract applies to each generated row kernel."
    )
    .unwrap();
    writeln!(f, "        unsafe {{").unwrap();
    for pi in 0..p {
        writeln!(
            f,
            "            encode_{d}x{p}_parity_{pi}_avx512(parity_rows, data, parity, shard_len);"
        )
        .unwrap();
    }
    writeln!(f, "        }}").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f).unwrap();

    for pi in 0..p {
        generate_encode_parity_fn_avx512(f, d, p, pi);
    }
}

fn generate_encode_parity_fn_avx512(f: &mut File, d: usize, p: usize, pi: usize) {
    use std::io::Write;

    writeln!(f, "    #[target_feature(enable = \"avx512f,avx512bw\")]").unwrap();
    writeln!(f, "    unsafe fn encode_{d}x{p}_parity_{pi}_avx512(").unwrap();
    writeln!(f, "        parity_rows: &[&[u8]],").unwrap();
    writeln!(f, "        data: &[&[u8]],").unwrap();
    writeln!(f, "        parity: &mut [&mut [u8]],").unwrap();
    writeln!(f, "        shard_len: usize,").unwrap();
    writeln!(f, "    ) {{").unwrap();
    writeln!(
        f,
        "        // SAFETY: the parent dispatcher checked AVX-512BW availability and slice lengths."
    )
    .unwrap();
    writeln!(f, "        unsafe {{").unwrap();
    writeln!(
        f,
        "            let nibble_mask: __m512i = _mm512_set1_epi8(0x0f);"
    )
    .unwrap();
    for di in 0..d {
        writeln!(
            f,
            "            let (coef_low_{di}, coef_high_{di}): (__m512i, __m512i) = {{"
        )
        .unwrap();
        writeln!(f, "                let c = parity_rows[{pi}][{di}];").unwrap();
        writeln!(
            f,
            "                let (lh, hh) = super::super::load_table_halves(c);"
        )
        .unwrap();
        writeln!(
            f,
            "                let low128: __m128i = _mm_loadu_si128(lh.as_ptr().cast());"
        )
        .unwrap();
        writeln!(
            f,
            "                let high128: __m128i = _mm_loadu_si128(hh.as_ptr().cast());"
        )
        .unwrap();
        writeln!(
            f,
            "                (_mm512_broadcast_i32x4(low128), _mm512_broadcast_i32x4(high128))"
        )
        .unwrap();
        writeln!(f, "            }};").unwrap();
    }
    writeln!(f).unwrap();
    writeln!(f, "            let bytes_done = shard_len & !63usize;").unwrap();
    writeln!(f, "            let mut offset = 0usize;").unwrap();
    writeln!(f, "            while offset < bytes_done {{").unwrap();
    writeln!(
        f,
        "                let data_vec: __m512i = _mm512_loadu_si512(data[0][offset..].as_ptr().cast());"
    )
    .unwrap();
    writeln!(
        f,
        "                let low = _mm512_and_si512(data_vec, nibble_mask);"
    )
    .unwrap();
    writeln!(
        f,
        "                let high = _mm512_and_si512(_mm512_srli_epi64::<4>(data_vec), nibble_mask);"
    )
    .unwrap();
    writeln!(
        f,
        "                let mut acc: __m512i = _mm512_xor_si512("
    )
    .unwrap();
    writeln!(
        f,
        "                    _mm512_shuffle_epi8(coef_low_0, low),"
    )
    .unwrap();
    writeln!(
        f,
        "                    _mm512_shuffle_epi8(coef_high_0, high),"
    )
    .unwrap();
    writeln!(f, "                );").unwrap();
    for di in 1..d {
        writeln!(
            f,
            "                let data_vec: __m512i = _mm512_loadu_si512(data[{di}][offset..].as_ptr().cast());"
        )
        .unwrap();
        writeln!(
            f,
            "                let low = _mm512_and_si512(data_vec, nibble_mask);"
        )
        .unwrap();
        writeln!(
            f,
            "                let high = _mm512_and_si512(_mm512_srli_epi64::<4>(data_vec), nibble_mask);"
        )
        .unwrap();
        writeln!(
            f,
            "                acc = _mm512_xor_si512(acc, _mm512_xor_si512("
        )
        .unwrap();
        writeln!(
            f,
            "                    _mm512_shuffle_epi8(coef_low_{di}, low),"
        )
        .unwrap();
        writeln!(
            f,
            "                    _mm512_shuffle_epi8(coef_high_{di}, high),"
        )
        .unwrap();
        writeln!(f, "                ));").unwrap();
    }
    writeln!(
        f,
        "                _mm512_storeu_si512(parity[{pi}][offset..].as_mut_ptr().cast(), acc);"
    )
    .unwrap();
    writeln!(f, "                offset += 64;").unwrap();
    writeln!(f, "            }}").unwrap();

    writeln!(f).unwrap();
    writeln!(f, "            for i in bytes_done..shard_len {{").unwrap();
    writeln!(f, "                let mut acc: u8 = 0;").unwrap();
    for di in 0..d {
        writeln!(f, "                acc ^= super::super::super::MUL_TABLE[parity_rows[{pi}][{di}] as usize][data[{di}][i] as usize];").unwrap();
    }
    writeln!(f, "                parity[{pi}][i] = acc;").unwrap();
    writeln!(f, "            }}").unwrap();
    writeln!(f, "        }}").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f).unwrap();
}

fn generate_encode_codegen_neon(f: &mut File, configs: &[(usize, usize)]) {
    use std::io::Write;

    writeln!(
        f,
        "// Auto-generated NEON encode functions for common configurations."
    )
    .unwrap();
    writeln!(f, "// Generated by build.rs — do not edit manually.").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "#[cfg(all(").unwrap();
    writeln!(f, "    feature = \"simd-neon\",").unwrap();
    writeln!(f, "    target_arch = \"aarch64\",").unwrap();
    writeln!(f, "    not(target_env = \"msvc\"),").unwrap();
    writeln!(
        f,
        "    not(any(target_os = \"android\", target_os = \"ios\"))"
    )
    .unwrap();
    writeln!(f, "))]").unwrap();
    writeln!(f, "mod neon_impl {{").unwrap();
    writeln!(f, "    use core::arch::aarch64::*;").unwrap();
    writeln!(f).unwrap();

    for &(d, p) in configs {
        generate_encode_fn_neon(f, d, p);
    }

    writeln!(f, "}}").unwrap();
    writeln!(f).unwrap();

    generate_encode_selector(f, configs, "neon");
}

fn generate_encode_fn_neon(f: &mut File, d: usize, p: usize) {
    use std::io::Write;

    writeln!(
        f,
        "    /// Encode {d}+{p} with one shared nibble extraction per input vector."
    )
    .unwrap();
    writeln!(f, "    ///").unwrap();
    writeln!(f, "    /// # Safety").unwrap();
    writeln!(
        f,
        "    /// The checked dispatcher must prove NEON availability, coefficient and buffer bounds;"
    )
    .unwrap();
    writeln!(
        f,
        "    /// the input/output views must remain borrowed throughout this invocation."
    )
    .unwrap();
    writeln!(f, "    #[target_feature(enable = \"neon\")]").unwrap();
    writeln!(f, "    pub(super) unsafe fn encode_{d}x{p}_neon(").unwrap();
    writeln!(
        f,
        "        parity_rows: &[&[u8]], data: &[&[u8]], parity: &mut [&mut [u8]], shard_len: usize,"
    )
    .unwrap();
    writeln!(f, "    ) {{").unwrap();
    writeln!(f, "        let nibble_mask = vdupq_n_u8(0x0f);").unwrap();
    for pi in 0..p {
        for di in 0..d {
            writeln!(
                f,
                "        // SAFETY: the parent proved NEON availability and coefficient-row bounds;"
            )
            .unwrap();
            writeln!(
                f,
                "        // both table rows contain 16 bytes and permit unaligned loads."
            )
            .unwrap();
            writeln!(
                f,
                "        let (coef_low_{pi}_{di}, coef_high_{pi}_{di}) = unsafe {{"
            )
            .unwrap();
            writeln!(f, "            let c = parity_rows[{pi}][{di}] as usize;").unwrap();
            writeln!(
                f,
                "            (vld1q_u8(super::super::super::MUL_TABLE_LOW[c].as_ptr()),"
            )
            .unwrap();
            writeln!(
                f,
                "             vld1q_u8(super::super::super::MUL_TABLE_HIGH[c].as_ptr()))"
            )
            .unwrap();
            writeln!(f, "        }};").unwrap();
        }
    }
    // The single checked invocation has validated all lengths. Capture pointers
    // once so the fixed-width vector loop does not repeat slice bounds checks.
    for di in 0..d {
        writeln!(f, "        let input_{di} = data[{di}].as_ptr();").unwrap();
    }
    for pi in 0..p {
        writeln!(f, "        let output_{pi} = parity[{pi}].as_mut_ptr();").unwrap();
    }
    writeln!(f, "        let bytes_done = shard_len & !15usize;").unwrap();
    writeln!(f, "        let mut offset = 0usize;").unwrap();
    writeln!(f, "        while offset < bytes_done {{").unwrap();
    writeln!(
        f,
        "            // SAFETY: offset is 16-byte aligned and offset+16 <= shard_len;"
    )
    .unwrap();
    writeln!(
        f,
        "            // pointers derive from live input/output borrows; the checked parent proves buffer bounds and NEON availability."
    )
    .unwrap();
    writeln!(f, "            unsafe {{").unwrap();
    // Keep parity accumulators fused: one input load and nibble extraction feed
    // every output. Streaming avoids retaining all input vectors at once.
    for di in 0..d {
        writeln!(
            f,
            "                let value = vld1q_u8(input_{di}.add(offset));"
        )
        .unwrap();
        writeln!(f, "                let low = vandq_u8(value, nibble_mask);").unwrap();
        writeln!(f, "                let high = vshrq_n_u8::<4>(value);").unwrap();
        for pi in 0..p {
            if di == 0 {
                writeln!(f, "                let mut acc_{pi} = veorq_u8(").unwrap();
            } else {
                writeln!(f, "                acc_{pi} = veorq_u8(acc_{pi}, veorq_u8(").unwrap();
            }
            writeln!(
                f,
                "                    vqtbl1q_u8(coef_low_{pi}_{di}, low),"
            )
            .unwrap();
            writeln!(
                f,
                "                    vqtbl1q_u8(coef_high_{pi}_{di}, high),"
            )
            .unwrap();
            writeln!(f, "                {}", if di == 0 { ");" } else { "));" }).unwrap();
        }
    }
    for pi in 0..p {
        writeln!(
            f,
            "                vst1q_u8(output_{pi}.add(offset), acc_{pi});"
        )
        .unwrap();
    }
    writeln!(f, "            }}").unwrap();
    writeln!(f, "            offset += 16;").unwrap();
    writeln!(f, "        }}").unwrap();
    writeln!(f, "        for i in bytes_done..shard_len {{").unwrap();
    for pi in 0..p {
        writeln!(f, "            let mut acc: u8 = 0;").unwrap();
        for di in 0..d {
            writeln!(f, "            acc ^= super::super::super::MUL_TABLE[parity_rows[{pi}][{di}] as usize][data[{di}][i] as usize];").unwrap();
        }
        writeln!(f, "            parity[{pi}][i] = acc;").unwrap();
    }
    writeln!(f, "        }}").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f).unwrap();
}

fn main() {
    // Named `cfg` aliases for the SIMD-backend gating in `galois_8/backend.rs`.
    // Each backend/arch predicate (feature + arch + platform exclusions) was
    // repeated ~a dozen times and had to be hand-synchronised whenever a backend
    // or architecture was added (e.g. the ppc64le VSX arm, #1244). Defining them
    // once here collapses those to `#[cfg(rse_*)]`. `cfg_aliases` emits the
    // matching `rustc-check-cfg` automatically. `std`-qualified variants stay
    // `all(rse_*, feature = "std")` at the use site.
    cfg_aliases::cfg_aliases! {
        // Per-family x86_64 backends (each needs SSSE3/AVX2/AVX512/GFNI and a
        // non-MSVC, non-android/ios platform for the Rust intrinsics path).
        rse_x86_ssse3: { all(feature = "simd-ssse3", target_arch = "x86_64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        rse_x86_avx2: { all(feature = "simd-avx2", target_arch = "x86_64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        rse_x86_avx512: { all(feature = "simd-avx512", target_arch = "x86_64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        rse_x86_gfni: { all(feature = "simd-gfni", target_arch = "x86_64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        // Any x86_64 rust-SIMD family.
        rse_x86_simd: { all(any(feature = "simd-ssse3", feature = "simd-avx2", feature = "simd-avx512", feature = "simd-gfni"), target_arch = "x86_64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        // aarch64 NEON and ppc64 VSX backends.
        rse_aarch64_neon: { all(feature = "simd-neon", target_arch = "aarch64", not(target_env = "msvc"), not(any(target_os = "android", target_os = "ios"))) },
        rse_ppc64_vsx: { all(feature = "simd-vsx", target_arch = "powerpc64") },
        // Any rust-SIMD backend on any supported architecture (the 3-arm block).
        rse_rust_simd: { any(rse_x86_simd, rse_aarch64_neon, rse_ppc64_vsx) },
    }

    write_tables();
    generate_encode_codegen();
}
