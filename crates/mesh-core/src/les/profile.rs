//! Spatial pilot-solver feedback with explicit side, units and flow provenance.

use super::feedback::fields;
use crate::{MeshConfiguration, MeshError, geometry::Point2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceSide {
    Upper,
    Lower,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallShearSample {
    pub side: SurfaceSide,
    pub x_over_chord: f64,
    pub wall_shear_stress_pa: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_wall_shear_stress_pa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intermittency: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_layer_thickness_chords: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileReference {
    pub airfoil: crate::config::AirfoilConfiguration,
    pub flow: crate::config::FlowConfiguration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallShearProfile {
    pub reference: ProfileReference,
    pub samples: Vec<WallShearSample>,
}

impl WallShearProfile {
    /// Rejects stale, incomplete or ambiguous data rather than extrapolating.
    /// # Errors
    /// Returns invalid-profile/provenance errors.
    pub fn validate(&self, configuration: &MeshConfiguration) -> Result<(), MeshError> {
        let current = reference(configuration);
        if self.reference != current {
            return Err(fail(
                "profile flow/airfoil provenance differs; import matching pilot data again",
            ));
        }
        if self.samples.len() > 100_000 {
            return Err(fail("profile exceeds 100000 samples"));
        }
        if self
            .samples
            .windows(2)
            .any(|s| s[0].side == SurfaceSide::Lower && s[1].side == SurfaceSide::Upper)
        {
            return Err(fail(
                "profile must contain the upper side followed by the lower side",
            ));
        }
        for side in [SurfaceSide::Upper, SurfaceSide::Lower] {
            let samples: Vec<_> = self.samples.iter().filter(|s| s.side == side).collect();
            if samples.len() < 2
                || samples[0].x_over_chord.abs() > 1e-12
                || (samples[samples.len() - 1].x_over_chord - 1.0).abs() > 1e-12
            {
                return Err(fail(
                    "each UPPER/LOWER side must cover xOverChord=0 to 1 with at least two samples",
                ));
            }
            if samples
                .windows(2)
                .any(|s| s[0].x_over_chord >= s[1].x_over_chord)
            {
                return Err(fail(
                    "profile coordinates must be unique and increasing on each side",
                ));
            }
        }
        for sample in &self.samples {
            if !sample.x_over_chord.is_finite()
                || !(0.0..=1.0).contains(&sample.x_over_chord)
                || !sample.wall_shear_stress_pa.is_finite()
                || sample.wall_shear_stress_pa < 0.0
            {
                return Err(fail(
                    "profile requires finite xOverChord in [0,1] and nonnegative magnitudes in Pa",
                ));
            }
            if sample.signed_wall_shear_stress_pa.is_some_and(|v| {
                !v.is_finite() || v.abs() > sample.wall_shear_stress_pa * (1.0 + 1e-6)
            }) {
                return Err(fail(
                    "signed tangential shear must be finite and no larger than the magnitude",
                ));
            }
            if sample
                .intermittency
                .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            {
                return Err(fail("solver intermittency must be in [0,1]"));
            }
            if sample
                .boundary_layer_thickness_chords
                .is_some_and(|v| !v.is_finite() || v <= 0.0)
            {
                return Err(fail(
                    "measured boundary-layer thickness must be positive in chord units",
                ));
            }
        }
        if self.maximum_shear() <= 0.0 {
            return Err(fail("profile contains no positive wall shear"));
        }
        Ok(())
    }

    #[must_use]
    pub fn maximum_shear(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.wall_shear_stress_pa)
            .fold(0.0, f64::max)
    }

    pub(crate) fn maximum_on_interval(&self, side: SurfaceSide, a: f64, b: f64) -> f64 {
        let low = a.min(b);
        let high = a.max(b);
        let mut maximum = self
            .sample(side, low)
            .wall_shear_stress_pa
            .max(self.sample(side, high).wall_shear_stress_pa);
        let split = self
            .samples
            .partition_point(|s| s.side == SurfaceSide::Upper);
        let points = if side == SurfaceSide::Upper {
            &self.samples[..split]
        } else {
            &self.samples[split..]
        };
        let start = points.partition_point(|s| s.x_over_chord < low);
        let end = points.partition_point(|s| s.x_over_chord <= high);
        for sample in &points[start..end] {
            maximum = maximum.max(sample.wall_shear_stress_pa);
        }
        maximum
    }

    /// Linear interpolation; the geometry's small nose overhang is clamped to x/c=0.
    #[must_use]
    pub(crate) fn sample(&self, side: SurfaceSide, x: f64) -> WallShearSample {
        let split = self
            .samples
            .partition_point(|s| s.side == SurfaceSide::Upper);
        let points = if side == SurfaceSide::Upper {
            &self.samples[..split]
        } else {
            &self.samples[split..]
        };
        let x = x.clamp(0.0, 1.0);
        let index = points
            .partition_point(|s| s.x_over_chord < x)
            .clamp(1, points.len() - 1);
        let a = &points[index - 1];
        let b = &points[index];
        let t = ((x - a.x_over_chord) / (b.x_over_chord - a.x_over_chord)).clamp(0.0, 1.0);
        let lerp = |u: f64, v: f64| u * (1.0 - t) + v * t;
        let optional = |u: Option<f64>, v: Option<f64>| u.zip(v).map(|(u, v)| lerp(u, v));
        WallShearSample {
            side,
            x_over_chord: x,
            wall_shear_stress_pa: lerp(a.wall_shear_stress_pa, b.wall_shear_stress_pa),
            signed_wall_shear_stress_pa: optional(
                a.signed_wall_shear_stress_pa,
                b.signed_wall_shear_stress_pa,
            ),
            intermittency: optional(a.intermittency, b.intermittency),
            boundary_layer_thickness_chords: optional(
                a.boundary_layer_thickness_chords,
                b.boundary_layer_thickness_chords,
            ),
        }
    }
}

/// Distributes existing wall counts by an arc-length/wall-unit monitor. Solver
/// intermittency and reverse shear increase density, never imply a predicted state.
pub(crate) fn redistribute_wall_positions(
    configuration: &MeshConfiguration,
    blocks: &mut [crate::topology::LogicalBlock2D],
) {
    use crate::topology::{BoundaryRole, ParametricCurve};
    let Some(profile) = &configuration.les.wall_shear_profile else {
        return;
    };
    let maximum = profile.maximum_shear();
    for block in blocks {
        if block.boundaries.bottom != Some(BoundaryRole::Airfoil) {
            continue;
        }
        let side = if block.name.starts_with("upper") {
            SurfaceSide::Upper
        } else {
            SurfaceSide::Lower
        };
        let mut cdf = vec![0.0];
        let mut previous = block.bottom.point(0.0);
        for index in 1..=512_u32 {
            let t = f64::from(index) / 512.0;
            let point = block.bottom.point(t);
            let midpoint = Point2::new(
                f64::midpoint(point.x, previous.x),
                f64::midpoint(point.y, previous.y),
            );
            let sample = profile.sample(side, chord_coordinate(midpoint, configuration));
            let transition = sample.intermittency.is_some_and(|v| v > 0.05 && v < 0.95);
            let reversed = sample.signed_wall_shear_stress_pa.is_some_and(|v| v < 0.0);
            let monitor = (sample.wall_shear_stress_pa / maximum).sqrt().max(0.25)
                * if transition || reversed { 2.0 } else { 1.0 };
            cdf.push(cdf[cdf.len() - 1] + point.distance_to(previous) * monitor);
            previous = point;
        }
        let total = cdf[cdf.len() - 1];
        for xi in block.xi_positions.iter_mut().skip(1).take(block.ni - 1) {
            let target = *xi * total;
            let index = cdf.partition_point(|v| *v < target).clamp(1, 512);
            #[allow(clippy::cast_precision_loss)]
            let t = (index - 1) as f64 + (target - cdf[index - 1]) / (cdf[index] - cdf[index - 1]);
            *xi = t / 512.0;
        }
    }
}

/// Chord-coordinate projection in the generator's positive-AoA convention.
#[must_use]
pub fn chord_coordinate(point: Point2, configuration: &MeshConfiguration) -> f64 {
    let chord = configuration.airfoil.chord_length;
    let angle = -configuration.airfoil.angle_of_attack_deg.to_radians();
    (0.25 + ((point.x - 0.25 * chord) * angle.cos() + point.y * angle.sin()) / chord)
        .clamp(0.0, 1.0)
}

/// CSV headers: side,xOverChord,wallShearStressPa and optional solver signals.
/// Empty optional fields are missing data, never inferred transition/separation.
/// # Errors
/// Rejects unsupported columns, malformed rows, incomplete coverage and invalid data.
pub fn import_wall_shear_profile(
    configuration: &MeshConfiguration,
    csv: &str,
) -> Result<MeshConfiguration, MeshError> {
    let mut lines = csv
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let headers = fields(lines.next().ok_or_else(|| fail("empty profile CSV"))?.1)?;
    let required = ["side", "xOverChord", "wallShearStressPa"];
    let optional = [
        "signedWallShearStressPa",
        "intermittency",
        "boundaryLayerThicknessChords",
    ];
    for name in required {
        if headers.iter().filter(|h| h.as_str() == name).count() != 1 {
            return Err(fail(&format!("expected exactly one {name} column")));
        }
    }
    for (i, name) in headers.iter().enumerate() {
        if (!required.contains(&name.as_str()) && !optional.contains(&name.as_str()))
            || headers[..i].contains(name)
        {
            return Err(fail("unsupported or duplicate profile column"));
        }
    }
    let mut samples = Vec::new();
    for (row, line) in lines {
        let values = fields(line)?;
        if values.len() != headers.len() {
            return Err(fail(&format!("line {}: wrong field count", row + 1)));
        }
        let value = |name: &str| {
            headers
                .iter()
                .position(|h| h == name)
                .map(|i| values[i].as_str())
        };
        let number = |name: &str| -> Result<Option<f64>, MeshError> {
            value(name)
                .filter(|v| !v.is_empty())
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|_| fail(&format!("line {}: invalid {name}", row + 1)))
                })
                .transpose()
        };
        let side = match value("side") {
            Some("UPPER") => SurfaceSide::Upper,
            Some("LOWER") => SurfaceSide::Lower,
            _ => return Err(fail("side must be UPPER or LOWER")),
        };
        samples.push(WallShearSample {
            side,
            x_over_chord: number("xOverChord")?.ok_or_else(|| fail("missing xOverChord"))?,
            wall_shear_stress_pa: number("wallShearStressPa")?
                .ok_or_else(|| fail("missing wallShearStressPa"))?,
            signed_wall_shear_stress_pa: number("signedWallShearStressPa")?,
            intermittency: number("intermittency")?,
            boundary_layer_thickness_chords: number("boundaryLayerThicknessChords")?,
        });
        if samples.len() > 100_000 {
            return Err(fail("profile exceeds 100000 samples"));
        }
    }
    samples.sort_by(|a, b| {
        (a.side == SurfaceSide::Lower)
            .cmp(&(b.side == SurfaceSide::Lower))
            .then(a.x_over_chord.total_cmp(&b.x_over_chord))
    });
    let profile = WallShearProfile {
        reference: reference(configuration),
        samples,
    };
    profile.validate(configuration)?;
    let mut candidate = configuration.clone();
    candidate.flow.reynolds_number = candidate.derived_reynolds_number();
    candidate.les.solver_wall_shear_stress = Some(profile.maximum_shear());
    candidate.les.estimate_first_cell_height = true;
    candidate.les.manual_first_cell_height = None;
    let maximum_thickness = profile
        .samples
        .iter()
        .filter_map(|s| s.boundary_layer_thickness_chords)
        .fold(
            candidate.les.reference_boundary_layer_thickness_chords,
            f64::max,
        );
    candidate.les.reference_boundary_layer_thickness_chords = maximum_thickness;
    candidate.les.wall_shear_profile = Some(profile);
    candidate.optimization.orthogonalize_wall = true;
    candidate.optimization.target_cell_center_y_plus = true;
    Ok(candidate)
}

fn reference(configuration: &MeshConfiguration) -> ProfileReference {
    let mut flow = configuration.flow.clone();
    flow.reynolds_number = configuration.derived_reynolds_number();
    let mut airfoil = configuration.airfoil.clone();
    // Geometry sampling density can change without invalidating the physical run.
    airfoil.surface_point_count = 0;
    ProfileReference { airfoil, flow }
}
fn fail(message: &str) -> MeshError {
    MeshError::FeedbackError(message.to_owned())
}
