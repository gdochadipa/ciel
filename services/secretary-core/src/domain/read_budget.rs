//! Trusted read-task quotas; no mutation approval or personal-memory state.
use serde::{Deserialize, Serialize};

/// Explicit cumulative limits selected by the operator; there are no defaults.
///
/// # Examples
/// ```
/// use secretary_core::domain::read_budget::ReadLimits;
/// let limits = ReadLimits { calls: 8, rows: 100, document_characters: 20000,
///     output_bytes: 262144, in_flight: 1 };
/// limits.validate()?;
/// # Ok::<(), &'static str>(())
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadLimits {
    /// Admitted plugin invocations, including failed/retried invocations.
    pub calls: u64,
    /// Returned spreadsheet records, including repeated emissions.
    pub rows: u64,
    /// Unicode scalars in all returned document string values, including metadata.
    pub document_characters: u64,
    /// Complete model-visible UTF-8 JSON text, with a reserved stop notice.
    pub output_bytes: u64,
    /// Maximum outstanding durable reservations.
    pub in_flight: u64,
}

impl ReadLimits {
    /// Validate finite integer limits within the supported local accounting range.
    ///
    /// # Errors
    /// Returns a static code for an unsupported or inconsistent limit.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=10000).contains(&self.calls)
            || self.rows > 1_000_000
            || self.document_characters > 100_000_000
            || !(512..=67_108_864).contains(&self.output_bytes)
            || !(1..=16).contains(&self.in_flight)
            || self.in_flight > self.calls
        {
            return Err("budget_limits_invalid");
        }
        Ok(())
    }

    pub(crate) fn within(&self, ceiling: &Self) -> bool {
        self.calls <= ceiling.calls
            && self.rows <= ceiling.rows
            && self.document_characters <= ceiling.document_characters
            && self.output_bytes <= ceiling.output_bytes
            && self.in_flight <= ceiling.in_flight
    }
}

/// Versioned protected policy for the initial read-only task runtime.
///
/// # Examples
/// ```no_run
/// # use secretary_core::domain::read_budget::ReadBudgetPolicy;
/// # fn check(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
/// let policy: ReadBudgetPolicy = serde_json::from_slice(bytes)?;
/// policy.validate()?;
/// # Ok(()) }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReadBudgetPolicy {
    /// Exact supported policy schema identifier.
    pub schema: String,
    /// Administrator revision; changing it requires a new deployment generation.
    pub revision: String,
    /// Operator-created task ceilings; requests cannot raise them.
    pub ceiling: ReadLimits,
    /// Maximum complete model-visible bytes per admitted invocation.
    pub request_output_bytes: u64,
}

impl ReadBudgetPolicy {
    /// Validate the policy without reading files or granting task authority.
    ///
    /// # Errors
    /// Returns a sanitized code on unknown schema or invalid finite limits.
    pub fn validate(&self) -> Result<(), &'static str> {
        self.ceiling.validate()?;
        if self.schema != "ciel.readonly.budget-policy.v1"
            || !super::service_config::revision(&self.revision)
            || !(256..=1_048_576).contains(&self.request_output_bytes)
            || self.request_output_bytes > self.ceiling.output_bytes - 256
        {
            return Err("budget_policy_invalid");
        }
        Ok(())
    }
}

/// Actual visible result units; bytes include the whole emitted JSON text.
///
/// # Examples
/// ```
/// use secretary_core::domain::read_budget::ReadUsage;
/// let usage = ReadUsage { rows: 1, output_bytes: 512, ..ReadUsage::default() };
/// assert_eq!(usage.document_characters, 0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadUsage {
    /// Returned spreadsheet rows.
    pub rows: u64,
    /// Document Unicode scalars, including visible metadata string values.
    pub document_characters: u64,
    /// Model-visible UTF-8 bytes.
    pub output_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_limits_preserve_stop_notice_capacity_and_reject_implicit_defaults() {
        let limits = ReadLimits {
            calls: 8,
            rows: 100,
            document_characters: 20000,
            output_bytes: 262144,
            in_flight: 1,
        };
        let mut policy = ReadBudgetPolicy {
            schema: "ciel.readonly.budget-policy.v1".into(),
            revision: "v1".into(),
            ceiling: limits,
            request_output_bytes: 65536,
        };
        assert!(policy.validate().is_ok());
        policy.request_output_bytes = policy.ceiling.output_bytes;
        assert!(policy.validate().is_err());
        policy.request_output_bytes = 256;
        policy.ceiling.in_flight = 0;
        assert!(policy.validate().is_err());
        assert!(serde_json::from_str::<ReadLimits>("{}").is_err());
    }
}
