pub const PUERTO_BASE_SHOP: usize = 6000;
pub const PUERTO_BASE_ECOMMERCE: usize = 7000;
pub const LOCAL_HOST: &str = "127.0.0.1:";

//Message Type
pub const SYN_TYPE: u8 = 0;
pub const TOKEN_TYPE: u8 = 1;
pub const OFFLINE_SHOP: u8 = 2;

// Oder delivery

pub const PROBABILITY_SUCCESS_DELIVERY: f64 = 0.5;
pub const MAX_TIME_DELIVERY: usize = 15;

//Op
pub const CONNECT: u8 = 99;
pub const DISCONNECT: u8 = 100;
