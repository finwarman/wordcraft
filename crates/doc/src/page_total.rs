//! One nested NUMPAGES field and an integer offset, retained across DOCX and layout.

use serde::{Deserialize, Serialize};

use crate::para::InlineObject;
use crate::props::CharProps;

// Matches the DOCX reader's field instruction cap, also for JSON-supplied objects.
const MAX_OFFSET_DIGITS: usize = 19;
const MAX_CODE: usize = 2 * 1024 * 1024;

/// Shared by adjacent DOCX and Layout layers; ordinary fields keep their existing model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NestedPageTotal {
    pub before: String,
    pub child: Box<InlineObject>,
    pub after: String,
    pub result: String,
    pub locked: bool,
    pub dirty: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
    #[serde(default, skip_serializing_if = "CharProps::is_empty")]
    pub child_props: CharProps,
}

impl NestedPageTotal {
    /// Accept the entire supported expression, including the child's identity and switches.
    pub fn offset(&self) -> Option<i64> {
        let InlineObject::Field { instr, result, form, .. } = &*self.child else { return None };
        let size = self.before.len().checked_add(instr.len())?.checked_add(self.after.len())?.checked_add(4)?;
        if size > MAX_CODE
            || self.result.len() > MAX_CODE
            || result.len() > MAX_CODE
            || self.child_props.ins.is_some()
            || self.child_props.del.is_some()
            || self.before.trim() != "="
        {
            return None;
        }
        if !instr.trim().eq_ignore_ascii_case("NUMPAGES") || !form.is_empty() {
            return None;
        }
        let tail = self.after.trim();
        let (negative, digits) = match tail.as_bytes().first()? {
            b'+' => (false, tail.get(1..)?.trim()),
            b'-' => (true, tail.get(1..)?.trim()),
            _ => return None,
        };
        if digits.is_empty() || digits.len() > MAX_OFFSET_DIGITS || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value = digits.parse::<i64>().ok()?;
        if negative { value.checked_neg() } else { Some(value) }
    }

    /// Code view uses the actual nested instruction, never the child's stale cache.
    pub fn instruction(&self) -> Option<String> {
        self.offset()?;
        let instr = match &*self.child {
            InlineObject::Field { instr, .. } => instr.as_str(),
            _ => "",
        };
        Some(format!("{}{{ {} }}{}", self.before, instr.trim(), self.after))
    }

    /// Locks and per-field code view prevent a live total from replacing the cached result.
    pub fn evaluate(&self, pages: u32) -> Option<String> {
        let InlineObject::Field { locked, code, .. } = &*self.child else { return None };
        if self.locked || *locked || *code {
            return None;
        }
        i64::from(pages).checked_add(self.offset()?).map(|value| value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(after: &str) -> NestedPageTotal {
        NestedPageTotal {
            before: "= ".into(),
            child: Box::new(InlineObject::Field {
                instr: "NUMPAGES".into(),
                result: "29".into(),
                locked: false,
                dirty: true,
                code: false,
                form: String::new(),
            }),
            after: after.into(),
            result: "26".into(),
            locked: false,
            dirty: true,
            code: false,
            child_props: CharProps::default(),
        }
    }

    #[test]
    fn arithmetic_is_checked() {
        for (suffix, expected) in [(" - 3", "17"), ("+0", "20"), (" + 4 ", "24"), ("-21", "-1")] {
            assert_eq!(total(suffix).evaluate(20).as_deref(), Some(expected));
        }
        assert_eq!(total("+9223372036854775807").evaluate(1), None);
        for suffix in ["", "3", "--3", "+-3", "+3 extra", "*3", "-3+1", "+٣", "+9223372036854775808"] {
            assert_eq!(total(suffix).offset(), None, "{suffix}");
        }
    }

    #[test]
    fn unsupported_stays_cached() {
        let mut field = total("-3");
        field.locked = true;
        assert_eq!(field.evaluate(20), None);
        field.locked = false;
        if let InlineObject::Field { locked, .. } = &mut *field.child {
            *locked = true;
        }
        assert_eq!(field.evaluate(20), None);
        if let InlineObject::Field { locked, code, .. } = &mut *field.child {
            *locked = false;
            *code = true;
        }
        assert_eq!(field.evaluate(20), None);
        if let InlineObject::Field { code, instr, .. } = &mut *field.child {
            *code = false;
            *instr = "NUMPAGES \\* Arabic".into();
        }
        assert_eq!(field.offset(), None);
        assert_eq!(field.result, "26");
    }

    #[test]
    fn model_inputs_are_bounded() {
        let mut field = total("-3");
        field.before = " ".repeat(MAX_CODE);
        assert_eq!(field.offset(), None);
        assert_eq!(field.instruction(), None);
        field.before = "= ".into();
        *field.child = InlineObject::FieldEnd;
        assert_eq!(field.evaluate(20), None);
        assert_eq!(field.result, "26");
    }
}
