use super::{AbstractMMU, MMUFlushMode, MMUTranslationResult};

pub struct NoMMU {}

impl AbstractMMU for NoMMU {
    fn new() -> Self {
        Self {}
    }
    fn translate_and_refill(
        &mut self,
        _core_id: u32,
        va: u64,
        _: u64,
        _: bool,
    ) -> MMUTranslationResult {
        MMUTranslationResult::Hit(va, 0)
    }

    fn lookup(&mut self, _: u64, _: u64, _: bool) -> Option<u64> {
        None
    }

    fn flush(&mut self, _mode: MMUFlushMode) {}

    fn serialize(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn deserialize(&mut self, _: serde_json::Value) {}
}
