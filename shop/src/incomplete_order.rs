use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::shop_error::ShopError;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct IncompleteOrder {
    product_id: String,
    quantity: usize,
    shop_id: usize,
}

impl IncompleteOrder {
    pub fn new(shop_id: usize, product_id: String, quantity: usize) -> Self {
        IncompleteOrder {
            product_id,
            quantity,
            shop_id,
        }
    }

    pub fn get_shop_id(&self) -> usize {
        self.shop_id
    }

    pub fn get_product_id(&self) -> &String {
        &self.product_id
    }

    pub fn get_quantity(&self) -> usize {
        self.quantity
    }

    pub fn serialize_to_bytes(&self) -> Result<Vec<u8>, ShopError> {
        let bytes = serde_json::to_string(&self)
            .map_err(|_| {
                ShopError::ToBytes("Fallo en serializar una incomplete order".to_string())
            })?
            .as_bytes()
            .to_vec();
        Ok([bytes.len().to_le_bytes().to_vec(), bytes].concat())
    }

    pub fn deserialize_from_bytes(stream: &mut dyn Read) -> Result<Self, ShopError> {
        let mut len_bytes = [0u8; 8];
        stream.read_exact(&mut len_bytes).map_err(|_| {
            ShopError::FromBytes("Fallo en deserializar una incomplete order".to_string())
        })?;
        let len = usize::from_le_bytes(len_bytes);

        let mut bytes = vec![0u8; len];
        stream.read_exact(&mut bytes).map_err(|_| {
            ShopError::FromBytes("Fallo en deserializar una incomplete order".to_string())
        })?;

        let order = serde_json::from_slice(&bytes).map_err(|_| {
            ShopError::FromBytes("Fallo en deserializar una incomplete order".to_string())
        })?;

        Ok(order)
    }
}
