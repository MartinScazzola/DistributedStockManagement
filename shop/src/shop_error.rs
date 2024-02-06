#[derive(Debug, PartialEq)]
pub enum ShopError {
    ParseOrder(String),
    Send(String),
    ParseArgs(String),
    OpenFile(String),
    SystemRun(String),
    ToBytes(String),
    FromBytes(String),
    Bind(String),
    Write(String),
    Read(String),
    Accept(String),
    ReceiveConnection(String),
    ConnectShop(String),
    InsufficientStock(String),
}
