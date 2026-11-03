//! The ITR NVSE plugin's pure functions: Sv_Join, Sv_Reverse and Sv_TrimStr
//! (docs/spec-notes/ext-itr.md §1). The wiki export has no page for any of the three, so
//! each behaviour is chosen from the name and the catalogue's parameters, as the VM's
//! own `sv_*` functions behave [hypothesis]. ITR is not installed in the probe game.

use super::{Vm, VmError, XVal};
use crate::ir::XCall;

impl Vm<'_, '_> {
    /// An ITR function by lower-case name; `None` for a name this module does not build.
    pub(super) fn itr_function(&mut self, n: &str, c: &XCall) -> Result<Option<XVal>, VmError> {
        if !matches!(n, "sv_join" | "sv_reverse" | "sv_trimstr") {
            return Ok(None);
        }
        let mut args = Vec::with_capacity(c.args.len());
        for a in &c.args {
            args.push(self.xeval(a)?);
        }
        let text = |k: usize| match args.get(k) {
            Some(XVal::Str(s)) => s.clone(),
            _ => String::new(),
        };
        Ok(Some(match n {
            // `array [delimiter]`: the array's values in its order, each as ToString writes
            // it, with the delimiter between them; no delimiter joins them as they are;
            // no array gives "" [hypothesis: no page; the catalogue's `array delimiter`].
            "sv_join" => {
                let values: Vec<XVal> = match args.first() {
                    Some(XVal::Array(id)) => self.store()?.array(*id).map(|a| a.entries.iter().map(|(_, v)| v.clone()).collect()).unwrap_or_default(),
                    _ => Vec::new(),
                };
                let mut parts = Vec::with_capacity(values.len());
                for v in &values {
                    parts.push(self.text_of(v)?);
                }
                XVal::Str(parts.join(&text(1)))
            }
            // The characters in reverse order [hypothesis: no page].
            "sv_reverse" => XVal::Str(text(0).chars().rev().collect()),
            // White space taken off both ends [hypothesis: no page; "trim" as the string
            // libraries use the word].
            _ => XVal::Str(text(0).trim().to_string()),
        }))
    }
}
