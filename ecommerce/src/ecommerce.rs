use rand::Rng;
use shop::order::Order;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    thread,
    time::Duration,
};

use crate::{
    ecommerce_const::{LOCAL_HOST, PUERTO_BASE},
    ecommerce_error::EcommerceError,
};

pub struct Ecommerce {
    shop_connections: HashMap<usize, TcpStream>,
}

impl Ecommerce {
    pub fn new() -> Ecommerce {
        Ecommerce {
            shop_connections: HashMap::new(),
        }
    }

    pub fn make_orders(&mut self, path: String, shop_amount: usize) -> Result<(), EcommerceError> {
        let file = File::open(path)
            .map_err(|_| EcommerceError::ReadFile("Failed reading file".to_string()))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            if let Ok(line) = line {
                let order = Order::parse_order(line).map_err(|_| {
                    EcommerceError::ParseOrder("Fail to parse ecommerce order".to_string())
                })?;
                loop {
                    match self.get_nearest_shop(shop_amount) {
                        Ok(shop_id) => {
                            let shop = self.shop_connections.get_mut(&shop_id).ok_or(
                                EcommerceError::ConnectShop("Fail to connect shop".to_string()),
                            )?;

                            if shop
                                .write_all(&order.serialize_to_bytes().map_err(|_| {
                                    EcommerceError::ToBytes(
                                        "Falla en serializar la orden".to_string(),
                                    )
                                })?)
                                .is_err()
                            {
                                println!("Fallo en conectarse a la tienda {}", shop_id);
                                if let Ok(connection) = TcpStream::connect(
                                    LOCAL_HOST.to_string() + &(PUERTO_BASE + shop_id).to_string(),
                                ) {
                                    self.shop_connections.insert(shop_id, connection);
                                }
                                thread::sleep(Duration::from_secs(2));
                            } else {
                                println!(
                                    "Se envio la orden:  {:?} a la tienda [TIENDA {}]\n",
                                    order, shop_id
                                );
                                break;
                            };
                        }
                        Err(err) => {
                            println!("{:?}", err);
                        }
                    };
                }
            }

            thread::sleep(Duration::from_secs(3));
        }
        Ok(())
    }

    pub fn get_nearest_shop(&mut self, shop_amount: usize) -> Result<usize, EcommerceError> {
        let mut rng = rand::thread_rng();
        let shop_id = rng.gen_range(0..shop_amount);

        if self.shop_connections.get_mut(&shop_id).is_some() {
            Ok(shop_id)
        } else if let Ok(connection) =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE + shop_id).to_string())
        {
            self.shop_connections.insert(shop_id, connection);
            Ok(shop_id)
        } else {
            return Err(EcommerceError::ConnectShop(format!(
                "Fallo en conectarse a la tienda {}",
                shop_id
            )));
        }
    }
}

impl Default for Ecommerce {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, thread};

    use super::*;

    #[test]
    fn test_get_nearest_shop() {
        let mut ecommerce = Ecommerce::new();
        let listener =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE + 0).to_string()).unwrap();

        let _handle = thread::spawn(move || {
            let mut stream = listener.accept().unwrap().0;
            let order = Order::new("1".to_string(), 1);
            let order_bytes = order
                .serialize_to_bytes()
                .expect("Failed to serialize order");
            stream.write_all(&order_bytes).unwrap();
        });

        let shop_id = ecommerce.get_nearest_shop(1).unwrap();
        assert_eq!(shop_id, 0);
    }
}
