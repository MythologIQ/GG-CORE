// Copyright 2024-2026 Veritas SDR Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bucket allocation functions for traffic splitting.

use std::hash::{DefaultHasher, Hash, Hasher};

use rand::Rng;

use crate::ab_testing::variant::VariantLabel;

/// Hash session ID to bucket 0-99.
pub fn hash_to_bucket(session_id: &str) -> u8 {
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    (hasher.finish() % 100) as u8
}

/// Generate random bucket 0-99 using cryptographically secure RNG.
pub fn random_bucket() -> u8 {
    rand::thread_rng().gen_range(0..100)
}

/// Convert bucket to variant using cumulative weights.
pub fn bucket_to_variant<'a>(
    bucket: u8,
    cumulative: &'a [(VariantLabel, u8)],
) -> &'a VariantLabel {
    for (label, threshold) in cumulative {
        if bucket < *threshold {
            return label;
        }
    }
    // Fallback (shouldn't happen with valid weights)
    &cumulative.last().unwrap().0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_to_bucket_deterministic() {
        let bucket1 = hash_to_bucket("session-123");
        let bucket2 = hash_to_bucket("session-123");
        assert_eq!(bucket1, bucket2);
    }

    #[test]
    fn test_hash_to_bucket_range() {
        for i in 0..100 {
            let bucket = hash_to_bucket(&format!("session-{}", i));
            assert!(bucket < 100);
        }
    }

    #[test]
    fn test_random_bucket_range() {
        for _ in 0..100 {
            let bucket = random_bucket();
            assert!(bucket < 100);
        }
    }

    #[test]
    fn test_bucket_to_variant_single() {
        let cumulative = vec![(VariantLabel::control(), 100)];
        for bucket in 0..100 {
            assert_eq!(
                bucket_to_variant(bucket, &cumulative),
                &VariantLabel::control()
            );
        }
    }

    #[test]
    fn test_bucket_to_variant_split() {
        let cumulative = vec![
            (VariantLabel::control(), 50),
            (VariantLabel::treatment(), 100),
        ];

        // Buckets 0-49 should map to control
        for bucket in 0..50 {
            assert_eq!(
                bucket_to_variant(bucket, &cumulative),
                &VariantLabel::control()
            );
        }

        // Buckets 50-99 should map to treatment
        for bucket in 50..100 {
            assert_eq!(
                bucket_to_variant(bucket, &cumulative),
                &VariantLabel::treatment()
            );
        }
    }

    #[test]
    fn test_bucket_to_variant_three_way() {
        let cumulative = vec![
            (VariantLabel::new("a"), 33),
            (VariantLabel::new("b"), 66),
            (VariantLabel::new("c"), 100),
        ];

        assert_eq!(
            bucket_to_variant(0, &cumulative),
            &VariantLabel::new("a")
        );
        assert_eq!(
            bucket_to_variant(32, &cumulative),
            &VariantLabel::new("a")
        );
        assert_eq!(
            bucket_to_variant(33, &cumulative),
            &VariantLabel::new("b")
        );
        assert_eq!(
            bucket_to_variant(65, &cumulative),
            &VariantLabel::new("b")
        );
        assert_eq!(
            bucket_to_variant(66, &cumulative),
            &VariantLabel::new("c")
        );
        assert_eq!(
            bucket_to_variant(99, &cumulative),
            &VariantLabel::new("c")
        );
    }
}
