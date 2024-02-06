#[derive(Debug)]
pub enum IncompleteOrderError {
    Deserialize(String),
}
