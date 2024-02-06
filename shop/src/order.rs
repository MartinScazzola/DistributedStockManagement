use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::shop_error::ShopError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Order {
    product_id: String,
    quantity: usize,
}

impl Order {
    pub fn new(product_id: String, quantity: usize) -> Self {
        Order {
            product_id,
            quantity,
        }
    }

    pub fn parse_order(line: String) -> Result<Self, ShopError> {
        let fields: Vec<&str> = line.split(": ").collect();
        let product_id = fields[0].to_string();
        let quantity: usize = fields[1]
            .parse()
            .map_err(|_| ShopError::ParseOrder("Fail to parse quantity".to_string()))?;

        Ok(Order::new(product_id, quantity))
    }

    pub fn get_product_id(&self) -> String {
        self.product_id.clone()
    }

    pub fn get_quantity(&self) -> usize {
        self.quantity
    }

    pub fn serialize_to_bytes(&self) -> Result<Vec<u8>, ShopError> {
        let bytes = serde_json::to_string(&self)
            .map_err(|_| ShopError::ToBytes("Falla en serializar la orden".to_string()))?
            .as_bytes()
            .to_vec();
        Ok([bytes.len().to_le_bytes().to_vec(), bytes].concat())
    }

    pub async fn deserialize_from_bytes(stream: &mut TcpStream) -> Result<Self, ShopError> {
        let mut len_bytes = [0u8; 8];
        stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(|_| ShopError::FromBytes(String::from("falla en deserializar la orden")))?;
        let len = usize::from_le_bytes(len_bytes);

        let mut bytes = vec![0u8; len];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|_| ShopError::FromBytes(String::from("falla en deserializar la orden")))?;

        let order = serde_json::from_slice(&bytes)
            .map_err(|_| ShopError::FromBytes(String::from("falla en deserializar la orden")))?;

        Ok(order)
    }
}
