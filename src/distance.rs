use half::f16;

pub const DIMS: usize = 14;

#[inline(always)]
pub fn euclidean_f16(a: &[f16], b: &[f16]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..DIMS {
        let av = a[i].to_f32();
        let bv = b[i].to_f32();
        let d = av - bv;
        sum += d * d;
    }
    sum.sqrt()
}

#[inline(always)]
pub fn euclidean_mixed(query: &[f32; DIMS], reference: &[f16]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..DIMS {
        let r = reference[i].to_f32();
        let d = query[i] - r;
        sum += d * d;
    }
    sum.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_have_zero_distance() {
        let v: Vec<f16> = [0.5f32; DIMS].iter().map(|&x| f16::from_f32(x)).collect();
        assert!((euclidean_f16(&v, &v) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn known_distance() {
        let a: Vec<f16> = [0.0f32; DIMS].iter().map(|&x| f16::from_f32(x)).collect();
        let mut b_vals = [0.0f32; DIMS];
        b_vals[0] = 1.0;
        let b: Vec<f16> = b_vals.iter().map(|&x| f16::from_f32(x)).collect();
        assert!((euclidean_f16(&a, &b) - 1.0).abs() < 0.01);
    }

    #[test]
    fn mixed_matches_f16() {
        let ref_vals: Vec<f16> = (0..DIMS)
            .map(|i| f16::from_f32(i as f32 * 0.1))
            .collect();
        let query: [f32; DIMS] = std::array::from_fn(|i| i as f32 * 0.05);

        let d1 = euclidean_mixed(&query, &ref_vals);

        let query_f16: Vec<f16> = query.iter().map(|&x| f16::from_f32(x)).collect();
        let d2 = euclidean_f16(&query_f16, &ref_vals);

        assert!((d1 - d2).abs() < 0.02);
    }
}
