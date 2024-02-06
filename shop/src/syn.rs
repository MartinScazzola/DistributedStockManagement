use serde::{Deserialize, Serialize};

use crate::{shop_const::SYN_TYPE, shop_error::ShopError};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Syn {
    id: usize,
}

impl Syn {
    pub fn new(id: usize) -> Syn {
        Syn { id }
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ShopError> {
        let json = serde_json::to_string(&self)
            .map_err(|_| ShopError::ToBytes("Fallo en serializar un syn".to_string()))?;
        let bytes = json.as_bytes().to_vec();

        Ok([vec![SYN_TYPE], bytes.len().to_le_bytes().to_vec(), bytes].concat())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Syn, ShopError> {
        let syn: Syn = serde_json::from_slice(bytes)
            .map_err(|_| ShopError::FromBytes("Fallo en deserializar un syn".to_string()))?;
        Ok(syn)
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::*;

    #[actix_rt::test]
    async fn serialize_and_deserialize_syn() -> Result<(), ShopError> {
        let syn = Syn::new(2);

        let bytes = syn.to_bytes()?;
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

        let syn_recv = Syn::from_bytes(stream)?;

        assert_eq!(msg_type, SYN_TYPE);
        assert_eq!(syn_recv, syn);
        assert_eq!(syn_recv.get_id(), 2);
        Ok(())
    }
}
