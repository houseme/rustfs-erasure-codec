use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn validate() -> Result<u32, JsValue> {
    let mut checks = 0u32;
    for (data, parity) in [(4, 3), (10, 4)] {
        let old = codec_old::galois_8::ReedSolomon::with_options(
            data,
            parity,
            codec_old::CodecOptions::builder()
                .codec_family(codec_old::CodecFamily::LeopardGF16)
                .build(),
        )
        .map_err(|e| JsValue::from_str(&format!("old constructor: {e:?}")))?;
        let new = codec_new::galois_8::ReedSolomon::with_options(
            data,
            parity,
            codec_new::CodecOptions::builder()
                .codec_family(codec_new::CodecFamily::LeopardGF16)
                .build(),
        )
        .map_err(|e| JsValue::from_str(&format!("new constructor: {e:?}")))?;
        for len in [64, 65_536, 65_600] {
            let source: Vec<Vec<u8>> = (0..data + parity)
                .map(|i| (0..len).map(|j| (i * 31 + j * 17 + j / 11) as u8).collect())
                .collect();
            let mut current = source.clone();
            new.encode(&mut current)
                .map_err(|e| JsValue::from_str(&format!("new encode {len}: {e:?}")))?;
            let mut previous = source.clone();
            old.encode(&mut previous)
                .map_err(|e| JsValue::from_str(&format!("old encode {len}: {e:?}")))?;
            if previous != current {
                return Err(JsValue::from_str(&format!(
                    "parity mismatch {data}+{parity} {len}"
                )));
            }
            checks += 1;
            for encoded in [&current, &previous] {
                for mode in 0..3 {
                    let mut missing: Vec<_> = encoded.iter().cloned().map(Some).collect();
                    missing[0] = None;
                    missing[data] = None;
                    let mut current_selected = missing.clone();
                    let mut previous_selected = missing;
                    let mut data_mask = vec![false; data];
                    data_mask[0] = true;
                    let mut parity_mask = vec![false; data + parity];
                    parity_mask[data] = true;
                    let current_result = match mode {
                        0 => new.reconstruct_data(&mut current_selected),
                        1 => new.reconstruct_some(&mut current_selected, &data_mask),
                        _ => new.reconstruct_some(&mut current_selected, &parity_mask),
                    };
                    current_result.map_err(|e| {
                        JsValue::from_str(&format!("new selective {len} mode {mode}: {e:?}"))
                    })?;
                    for (i, actual) in current_selected.iter().enumerate() {
                        let unrequested = (i == 0 && mode == 2) || (i == data && mode != 2);
                        let expected = if unrequested { None } else { Some(&encoded[i]) };
                        if actual.as_ref() != expected {
                            return Err(JsValue::from_str(&format!(
                                "new selective mismatch {data}+{parity} {len} mode {mode} shard {i}"
                            )));
                        }
                    }
                    checks += 1;

                    // Published 9.0.0 recovers all Leopard shards for both
                    // selective APIs. Preserve and explicitly check that
                    // historical behavior instead of equating it with 9.1.0.
                    let old_result = match mode {
                        0 => old.reconstruct_data(&mut previous_selected),
                        1 => old.reconstruct_some(&mut previous_selected, &data_mask),
                        _ => old.reconstruct_some(&mut previous_selected, &parity_mask),
                    };
                    old_result.map_err(|e| {
                        JsValue::from_str(&format!("old selective {len} mode {mode}: {e:?}"))
                    })?;
                    if previous_selected
                        .iter()
                        .zip(encoded)
                        .any(|(actual, expected)| actual.as_ref() != Some(expected))
                    {
                        return Err(JsValue::from_str(&format!(
                            "old selective mismatch {data}+{parity} {len} mode {mode}"
                        )));
                    }
                    checks += 1;
                }
                for missing_parity in [false, true] {
                    let mut lost: Vec<_> = encoded.iter().cloned().map(Some).collect();
                    lost[0] = None;
                    if missing_parity {
                        lost[data] = None;
                    }
                    let mut result = lost.clone();
                    new.reconstruct(&mut result)
                        .map_err(|e| JsValue::from_str(&format!("new decode {len}: {e:?}")))?;
                    if result
                        .iter()
                        .zip(encoded)
                        .any(|(v, expected)| v.as_ref() != Some(expected))
                    {
                        return Err(JsValue::from_str(&format!(
                            "new decode mismatch {data}+{parity} {len}"
                        )));
                    }
                    checks += 1;
                    old.reconstruct(&mut lost)
                        .map_err(|e| JsValue::from_str(&format!("old decode {len}: {e:?}")))?;
                    if lost
                        .iter()
                        .zip(encoded)
                        .any(|(v, expected)| v.as_ref() != Some(expected))
                    {
                        return Err(JsValue::from_str(&format!(
                            "old decode mismatch {data}+{parity} {len}"
                        )));
                    }
                    checks += 1;
                }
            }
        }
    }
    Ok(checks)
}
