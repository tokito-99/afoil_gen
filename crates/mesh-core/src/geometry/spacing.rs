//! Reusable one-dimensional point distributions.

use std::f64::consts::PI;

use crate::error::MeshError;

/// Returns `point_count` cosine-clustered values spanning exactly `[0, 1]`.
///
/// Clustering follows `x = 0.5 * (1 - cos(beta))`, with equally spaced
/// `beta` values. At least two points are required.
///
/// # Errors
///
/// Returns [`MeshError::GeometryError`] when fewer than two points are requested.
pub fn cosine_clustered(point_count: usize) -> Result<Vec<f64>, MeshError> {
    if point_count < 2 {
        return Err(MeshError::GeometryError(
            "cosine spacing requires at least two points".to_owned(),
        ));
    }

    #[allow(clippy::cast_precision_loss)]
    let interval_count = (point_count - 1) as f64;
    let mut values = Vec::with_capacity(point_count);
    for index in 0..point_count {
        #[allow(clippy::cast_precision_loss)]
        let beta = PI * index as f64 / interval_count;
        values.push(0.5 * (1.0 - beta.cos()));
    }

    // Pin analytical endpoints to make serialized output and endpoint tests exact.
    values[0] = 0.0;
    values[point_count - 1] = 1.0;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::cosine_clustered;

    #[test]
    fn spacing_is_monotonic_and_pins_endpoints() {
        let values = cosine_clustered(41).expect("valid point count");
        assert_eq!(values.first().copied(), Some(0.0));
        assert_eq!(values.last().copied(), Some(1.0));
        assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(values[1] - values[0] < values[20] - values[19]);
    }

    #[test]
    fn spacing_rejects_too_few_points() {
        assert!(cosine_clustered(1).is_err());
    }
}
