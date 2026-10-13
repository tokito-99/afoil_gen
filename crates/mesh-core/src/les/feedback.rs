//! Explicit, unit-safe pilot-solver feedback. No solver is run by this module.

use serde::{Deserialize, Serialize};

use crate::{MeshConfiguration, MeshError, ValidationReport, validate_configuration};

/// Statistics of nonnegative wall-shear magnitudes, in Pa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallShearFeedback {
    pub column: String,
    pub sample_count: usize,
    pub minimum_pa: f64,
    pub maximum_pa: f64,
    pub mean_pa: f64,
    pub selected_pa: f64,
    pub percentile: f64,
}

/// Configuration candidate and validation after applying solver feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibratedConfiguration {
    pub configuration: MeshConfiguration,
    pub feedback: WallShearFeedback,
    pub validation: ValidationReport,
}

/// Parses a comma-separated table with an explicit magnitude column in Pa.
/// Supports quoted fields and escaped quotes, but not multiline fields.
/// Every nonblank row must have the header's width and a finite nonnegative
/// magnitude. Percentiles use the nearest-rank rule; 100 selects the maximum.
///
/// # Errors
/// Rejects malformed tables, ambiguous columns, invalid samples and all-zero data.
pub fn parse_wall_shear_csv(
    csv: &str,
    column: &str,
    percentile: f64,
) -> Result<WallShearFeedback, MeshError> {
    if !percentile.is_finite() || percentile <= 0.0 || percentile > 100.0 {
        return Err(feedback_error("percentile must be in (0, 100]"));
    }
    let mut rows = csv
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, header) = rows.next().ok_or_else(|| feedback_error("CSV is empty"))?;
    let headers = fields(header)?;
    let columns: Vec<_> = headers
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_str() == column)
        .map(|(i, _)| i)
        .collect();
    if columns.len() != 1 {
        return Err(feedback_error(&format!(
            "expected exactly one column named {column:?}; values must be magnitudes in Pa"
        )));
    }
    let mut samples = Vec::new();
    for (line_index, row) in rows {
        let values = fields(row)
            .map_err(|error| feedback_error(&format!("line {}: {error}", line_index + 1)))?;
        if values.len() != headers.len() {
            return Err(feedback_error(&format!(
                "line {}: wrong number of fields",
                line_index + 1
            )));
        }
        let value = values[columns[0]].parse::<f64>().map_err(|_| {
            feedback_error(&format!(
                "line {}: invalid wall-shear magnitude",
                line_index + 1
            ))
        })?;
        if !value.is_finite() || value < 0.0 {
            return Err(feedback_error(&format!(
                "line {}: magnitude must be finite and nonnegative",
                line_index + 1
            )));
        }
        samples.push(value);
    }
    if samples.is_empty() {
        return Err(feedback_error("CSV has no samples"));
    }
    samples.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let index =
        ((percentile * samples.len() as f64 / 100.0).ceil() as usize).clamp(1, samples.len()) - 1;
    let selected_pa = samples[index];
    if selected_pa <= 0.0 {
        return Err(feedback_error(
            "selected wall shear is zero; choose a positive percentile sample",
        ));
    }
    // Sorted nonnegative data keep sample - mean finite, even near f64::MAX.
    #[allow(clippy::cast_precision_loss)]
    let mean_pa = samples
        .iter()
        .enumerate()
        .fold(0.0, |mean, (index, sample)| {
            mean + (sample - mean) / (index + 1) as f64
        });
    Ok(WallShearFeedback {
        column: column.to_owned(),
        sample_count: samples.len(),
        minimum_pa: samples[0],
        maximum_pa: samples[samples.len() - 1],
        mean_pa,
        selected_pa,
        percentile,
    })
}

/// Applies feedback without changing mesh counts, target wall units or flow.
/// Validation may fail if the existing layer budget cannot resolve this shear.
///
/// # Errors
/// Returns CSV parsing errors. Inspect `validation` before generating a mesh.
pub fn calibrate_configuration(
    configuration: &MeshConfiguration,
    csv: &str,
    column: &str,
    percentile: f64,
) -> Result<CalibratedConfiguration, MeshError> {
    let feedback = parse_wall_shear_csv(csv, column, percentile)?;
    let mut configuration = configuration.clone();
    configuration.flow.reynolds_number = configuration.derived_reynolds_number();
    configuration.les.estimate_first_cell_height = true;
    configuration.les.solver_wall_shear_stress = Some(feedback.selected_pa);
    configuration.les.manual_first_cell_height = None;
    configuration.les.wall_shear_profile = None;
    let validation = validate_configuration(&configuration);
    Ok(CalibratedConfiguration {
        configuration,
        feedback,
        validation,
    })
}

fn feedback_error(message: &str) -> MeshError {
    MeshError::FeedbackError(message.to_owned())
}

pub(crate) fn fields(line: &str) -> Result<Vec<String>, MeshError> {
    let mut result = Vec::new();
    let mut characters = line.chars().peekable();
    loop {
        while characters.peek().is_some_and(|c| matches!(c, ' ' | '\t')) {
            characters.next();
        }
        let mut value = String::new();
        if characters.peek() == Some(&'"') {
            characters.next();
            loop {
                match characters.next() {
                    Some('"') if characters.peek() == Some(&'"') => {
                        characters.next();
                        value.push('"');
                    }
                    Some('"') => break,
                    Some(character) => value.push(character),
                    None => {
                        return Err(feedback_error(
                            "unclosed quoted field; multiline fields are unsupported",
                        ));
                    }
                }
            }
            while characters
                .peek()
                .is_some_and(|c| matches!(c, ' ' | '\t' | '\r'))
            {
                characters.next();
            }
            if characters.peek().is_some_and(|c| *c != ',') {
                return Err(feedback_error("unexpected text after quoted field"));
            }
        } else {
            while characters.peek().is_some_and(|c| *c != ',') {
                let character = characters.next().expect("peeked character");
                if character == '"' {
                    return Err(feedback_error("quote inside unquoted field"));
                }
                value.push(character);
            }
        }
        result.push(value.trim().to_owned());
        if characters.next().is_none() {
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Selection and unchanged fields must match exactly.
mod tests {
    use super::*;

    #[test]
    fn accepts_bom_quotes_and_zero_samples() {
        let data = "\u{feff}\"x, coordinate\",\"wallShearStressPa\"\r\n0,0\r\n1,8\n2,2\n";
        let report = parse_wall_shear_csv(data, "wallShearStressPa", 100.0).unwrap();
        assert_eq!(report.sample_count, 3);
        assert_eq!(report.selected_pa, 8.0);
        assert_eq!(report.minimum_pa, 0.0);
        assert_eq!(
            parse_wall_shear_csv(data, "wallShearStressPa", 50.0)
                .unwrap()
                .selected_pa,
            2.0
        );
    }

    #[test]
    fn rejects_bad_feedback_without_skipping_rows() {
        for data in [
            "tau\n",
            "tau\n0",
            "tau\n-1",
            "tau\nNaN",
            "tau\ninf",
            "tau\n1\nno",
            "tau\n1,2",
            "tau,tau\n1,2",
            "\"tau\n1",
        ] {
            assert!(parse_wall_shear_csv(data, "tau", 100.0).is_err(), "{data}");
        }
        for percentile in [0.0, -1.0, 101.0, f64::NAN] {
            assert!(parse_wall_shear_csv("tau\n1", "tau", percentile).is_err());
        }
    }

    #[test]
    fn percentile_rank_and_statistics_stay_finite() {
        let rows = (1..=100)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            parse_wall_shear_csv(&format!("tau\n{rows}"), "tau", 7.0)
                .unwrap()
                .selected_pa,
            7.0
        );
        let report = parse_wall_shear_csv(
            "tau\n1.7976931348623157e308\n1.7976931348623157e308",
            "tau",
            100.0,
        )
        .unwrap();
        assert!(report.mean_pa.is_finite());
    }

    #[test]
    fn calibration_preserves_budget_and_targets() {
        let original = crate::test_support::example_configuration();
        let result = calibrate_configuration(&original, "tau\n8", "tau", 100.0).unwrap();
        assert_eq!(result.configuration.mesh, original.mesh);
        assert_eq!(
            result.configuration.les.target_y_plus,
            original.les.target_y_plus
        );
        assert_eq!(result.configuration.les.solver_wall_shear_stress, Some(8.0));
        assert_eq!(
            result.configuration.flow.reynolds_number,
            original.derived_reynolds_number()
        );
    }
}
