use serde::{Deserialize, Serialize};

use crate::{shop_const::OFFLINE_SHOP, shop_error::ShopError};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OfflineShop {
    offline_id: usize,
    replace_id: usize,
}

impl OfflineShop {
    pub fn new(offline_id: usize, replace_id: usize) -> OfflineShop {
        OfflineShop {
            offline_id,
            replace_id,
        }
    }

    pub fn get_offline_id(&self) -> usize {
        self.offline_id
    }

    pub fn get_replace_id(&self) -> usize {
        self.replace_id
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ShopError> {
        let json = serde_json::to_string(&self).map_err(|_| {
            ShopError::ToBytes("Fallo en pasar el offline shop a json string".to_string())
        })?;
        let bytes = json.as_bytes().to_vec();

        Ok([
            vec![OFFLINE_SHOP],
            bytes.len().to_le_bytes().to_vec(),
            bytes,
        ]
        .concat())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<OfflineShop, ShopError> {
        let offline_shop: OfflineShop = serde_json::from_slice(bytes).map_err(|_| {
            ShopError::ToBytes("falla en pasar de bytes a offline shop".to_string())
        })?;
        Ok(offline_shop)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[actix_rt::test]
    async fn serialize_and_deserialize_offline_shop() -> Result<(), ShopError> {
        let offline = OfflineShop::new(0, 1);

        let bytes = offline.to_bytes()?;
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

        let offline_recv = OfflineShop::from_bytes(stream)?;

        assert_eq!(msg_type, OFFLINE_SHOP);
        assert_eq!(offline_recv, offline);
        assert_eq!(offline_recv.get_offline_id(), 0);
        assert_eq!(offline_recv.get_replace_id(), 1);
        Ok(())
    }
}
