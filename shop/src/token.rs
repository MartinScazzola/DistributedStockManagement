use serde::{Deserialize, Serialize};

use crate::{incomplete_order::IncompleteOrder, shop_const::TOKEN_TYPE, shop_error::ShopError};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Token {
    id: usize,
    incomplete_orders: Vec<IncompleteOrder>,
}

impl Token {
    pub fn new(id: usize) -> Token {
        Token {
            id,
            incomplete_orders: vec![],
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ShopError> {
        let json = serde_json::to_string(&self)
            .map_err(|_| ShopError::ToBytes("Fallo en serializar el token".to_string()))?;
        let bytes = json.as_bytes().to_vec();

        Ok([vec![TOKEN_TYPE], bytes.len().to_le_bytes().to_vec(), bytes].concat())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Token, ShopError> {
        let token: Token = serde_json::from_slice(bytes)
            .map_err(|_| ShopError::FromBytes("Fallo en deserializar el token".to_string()))?;
        Ok(token)
    }

    pub fn increment_id(&mut self) {
        self.id += 1;
    }

    pub fn get_incomplete_orders(&self) -> &Vec<IncompleteOrder> {
        &self.incomplete_orders
    }

    pub fn set_incomplete_orders(&mut self, incomplete_orders: Vec<IncompleteOrder>) {
        self.incomplete_orders = incomplete_orders;
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[actix_rt::test]
    async fn serialize_and_deserialize_token() -> Result<(), ShopError> {
        let mut token = Token::new(7);
        let incomplete_orders = vec![
            IncompleteOrder::new(1, "p1".to_string(), 3),
            IncompleteOrder::new(2, "p2".to_string(), 5),
        ];

        token.set_incomplete_orders(incomplete_orders.clone());

        let bytes = token.to_bytes()?;
        let mut stream = bytes.as_slice();

        let mut type_bytes = [0u8; 1];
        stream
            .read_exact(&mut type_bytes)
            .await
            .map_err(|_| ShopError::Read("Failed to read".to_string()))?;
        let msg_type = type_bytes[0];

        let mut len_bytes = [0u8; 8];
        stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(|_| ShopError::Read("Failed to read".to_string()))?;
        let _len = usize::from_le_bytes(len_bytes);

        let token_recv = Token::from_bytes(stream)?;

        assert_eq!(msg_type, TOKEN_TYPE);
        assert_eq!(token_recv, token);
        assert_eq!(token_recv.get_incomplete_orders(), &incomplete_orders);
        Ok(())
    }
}
