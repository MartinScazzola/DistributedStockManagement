#[derive(Debug)]
pub enum EcommerceError {
    ConnectShop(String),
    ReadFile(String),
    ParseOrder(String),
    ParseArgs(String),
    ToBytes(String),
}
