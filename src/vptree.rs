use half::f16;

use crate::distance::{euclidean_f16, euclidean_mixed, DIMS};

#[derive(Clone, Copy)]
pub struct RefEntry {
    pub vector: [f16; DIMS],
    pub label: bool,
}

pub struct Neighbors {
    dists: [f32; 5],
    frauds: [bool; 5],
    count: usize,
    cached_worst: f32,
}

impl Neighbors {
    pub fn new() -> Self {
        Self {
            dists: [f32::INFINITY; 5],
            frauds: [false; 5],
            count: 0,
            cached_worst: f32::INFINITY,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, dist: f32, is_fraud: bool) {
        if self.count < 5 {
            self.dists[self.count] = dist;
            self.frauds[self.count] = is_fraud;
            self.count += 1;
            if self.count == 5 {
                self.refresh_worst();
            }
        } else if dist < self.cached_worst {
            let idx = self.worst_idx();
            self.dists[idx] = dist;
            self.frauds[idx] = is_fraud;
            self.refresh_worst();
        }
    }

    #[inline(always)]
    pub fn threshold(&self) -> f32 {
        self.cached_worst
    }

    pub fn fraud_count(&self) -> usize {
        self.frauds[..self.count].iter().filter(|&&f| f).count()
    }

    fn refresh_worst(&mut self) {
        let mut max = f32::NEG_INFINITY;
        for i in 0..self.count {
            if self.dists[i] > max {
                max = self.dists[i];
            }
        }
        self.cached_worst = max;
    }

    fn worst_idx(&self) -> usize {
        let mut idx = 0;
        let mut max = self.dists[0];
        for i in 1..self.count {
            if self.dists[i] > max {
                max = self.dists[i];
                idx = i;
            }
        }
        idx
    }
}

pub fn build_vptree(entries: &mut [RefEntry], medians: &mut [f32]) {
    build_recursive(entries, medians, 0, entries.len());
}

fn build_recursive(entries: &mut [RefEntry], medians: &mut [f32], lo: usize, hi: usize) {
    if hi <= lo + 1 {
        return;
    }

    let vp = entries[lo].vector;
    let count = hi - lo - 1;
    let k = count / 2;
    let mid = lo + 1 + k;

    entries[lo + 1..hi].select_nth_unstable_by(k, |a, b| {
        let da = euclidean_f16(&vp, &a.vector);
        let db = euclidean_f16(&vp, &b.vector);
        da.partial_cmp(&db).unwrap()
    });

    medians[lo] = euclidean_f16(&vp, &entries[mid].vector);

    build_recursive(entries, medians, lo + 1, mid);
    build_recursive(entries, medians, mid, hi);
}

pub fn query_knn(
    query: &[f32; DIMS],
    vectors: &[f16],
    labels: &[u8],
    medians: &[f32],
    n: usize,
) -> (f64, bool) {
    let mut neighbors = Neighbors::new();
    query_recursive(query, vectors, labels, medians, 0, n, &mut neighbors);
    let fraud_count = neighbors.fraud_count();
    let fraud_score = fraud_count as f64 / 5.0;
    let approved = fraud_score < 0.6;
    (fraud_score, approved)
}

fn query_recursive(
    query: &[f32; DIMS],
    vectors: &[f16],
    labels: &[u8],
    medians: &[f32],
    lo: usize,
    hi: usize,
    neighbors: &mut Neighbors,
) {
    if lo >= hi {
        return;
    }

    let ref_vec = &vectors[lo * DIMS..(lo + 1) * DIMS];
    let dist = euclidean_mixed(query, ref_vec);

    neighbors.push(dist, labels[lo] == 1);

    if hi - lo <= 1 {
        return;
    }

    let count = hi - lo - 1;
    let k = count / 2;
    let mid = lo + 1 + k;
    let median = medians[lo];

    if dist < median {
        query_recursive(query, vectors, labels, medians, lo + 1, mid, neighbors);
        if dist + neighbors.threshold() >= median {
            query_recursive(query, vectors, labels, medians, mid, hi, neighbors);
        }
    } else {
        query_recursive(query, vectors, labels, medians, mid, hi, neighbors);
        if dist - neighbors.threshold() <= median {
            query_recursive(query, vectors, labels, medians, lo + 1, mid, neighbors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(vals: [f32; DIMS], fraud: bool) -> RefEntry {
        let mut vector = [f16::ZERO; DIMS];
        for i in 0..DIMS {
            vector[i] = f16::from_f32(vals[i]);
        }
        RefEntry {
            vector,
            label: fraud,
        }
    }

    #[test]
    fn neighbors_keeps_top_5() {
        let mut n = Neighbors::new();
        for i in 0..10 {
            n.push(i as f32, i % 2 == 0);
        }
        assert_eq!(n.count, 5);
        assert!(n.threshold() < 5.0);
    }

    #[test]
    fn neighbors_replaces_worst() {
        let mut n = Neighbors::new();
        n.push(10.0, false);
        n.push(20.0, true);
        n.push(30.0, false);
        n.push(40.0, true);
        n.push(50.0, false);
        assert!((n.threshold() - 50.0).abs() < 0.001);

        n.push(5.0, true);
        assert!((n.threshold() - 40.0).abs() < 0.001);
        assert_eq!(n.fraud_count(), 3);
    }

    #[test]
    fn build_and_query_small_tree() {
        let base = [0.0f32; DIMS];

        let mut entries = Vec::new();
        for i in 0..20 {
            let mut vals = base;
            vals[0] = i as f32 * 0.05;
            entries.push(make_entry(vals, i >= 15));
        }

        let n = entries.len();
        let mut medians = vec![0.0f32; n];
        build_vptree(&mut entries, &mut medians);

        let mut vectors: Vec<f16> = Vec::with_capacity(n * DIMS);
        let mut labels: Vec<u8> = Vec::with_capacity(n);
        for e in &entries {
            vectors.extend_from_slice(&e.vector);
            labels.push(e.label as u8);
        }

        let mut query = [0.0f32; DIMS];
        query[0] = 0.01;
        let (score, approved) = query_knn(&query, &vectors, &labels, &medians, n);

        assert!(approved, "low-value query should be approved");
        assert!(score < 0.6, "fraud score should be low");
    }

    #[test]
    fn query_finds_fraudulent_neighbors() {
        let base = [0.0f32; DIMS];

        let mut entries = Vec::new();
        for i in 0..20 {
            let mut vals = base;
            vals[0] = i as f32 * 0.05;
            entries.push(make_entry(vals, i >= 5));
        }

        let n = entries.len();
        let mut medians = vec![0.0f32; n];
        build_vptree(&mut entries, &mut medians);

        let mut vectors: Vec<f16> = Vec::with_capacity(n * DIMS);
        let mut labels: Vec<u8> = Vec::with_capacity(n);
        for e in &entries {
            vectors.extend_from_slice(&e.vector);
            labels.push(e.label as u8);
        }

        let mut query = [0.0f32; DIMS];
        query[0] = 0.90;
        let (score, approved) = query_knn(&query, &vectors, &labels, &medians, n);

        assert!(!approved, "high-value query near frauds should be denied");
        assert!(score >= 0.6, "fraud score should be high, got {}", score);
    }
}
