use uuid::Uuid;

pub trait IdGenerator: Send + Sync {
    fn next_id(&self) -> Uuid;
}

#[derive(Debug, Default)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn next_id(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[cfg(test)]
mod tests {
    use super::{IdGenerator, UuidV7Generator};

    #[test]
    fn 產生不重複的第七版識別碼() {
        let generator = UuidV7Generator;
        let first = generator.next_id();
        let second = generator.next_id();
        assert_eq!(first.get_version_num(), 7);
        assert_ne!(first, second);
    }
}
