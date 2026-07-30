//! Ids are UUIDv7 text (§6.1 rule 4): they sort by creation time, which gives
//! a free chronological index, and two devices generating rows offline never
//! collide (§6.11).

use uuid::Uuid;

use crate::error::{AppError, Result};

pub fn new_id() -> String {
    Uuid::now_v7().to_string()
}

/// The renderer is not a trusted caller — every id crossing IPC is shape-checked.
pub fn validate_id(id: &str, what: &'static str) -> Result<()> {
    match Uuid::parse_str(id) {
        Ok(_) => Ok(()),
        Err(_) => Err(AppError::invalid(format!("'{id}' is not a valid {what} id."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v7_ids_sort_by_creation_order() {
        let mut ids: Vec<String> = (0..50)
            .map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(1));
                new_id()
            })
            .collect();
        let expected = ids.clone();
        ids.sort();
        assert_eq!(ids, expected);
    }

    #[test]
    fn rejects_junk() {
        assert!(validate_id("'; DROP TABLE task; --", "task").is_err());
    }
}
