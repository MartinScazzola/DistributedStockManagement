use std::sync::{mpsc::Receiver, Arc};

extern crate colored;

use colored::*;

use crate::{
    incomplete_order::IncompleteOrder,
    inventory::{Inventory, ProcessEcommerceOrder},
    offline_shop::OfflineShop,
    order::Order,
    shop_const::{
        LOCAL_HOST, OFFLINE_SHOP, PUERTO_BASE_ECOMMERCE, PUERTO_BASE_SHOP, SYN_TYPE, TOKEN_TYPE,
    },
    shop_error::ShopError,
    syn::Syn,
    token::Token,
};
use actix::Addr;
use async_std::channel::Sender;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
    time::{self, Duration},
};
#[derive(Debug)]
pub struct Shop {
    id: usize,
    total_shops: usize,
    shop_listener: Arc<Mutex<TcpListener>>,
    ecommerce_listener: Arc<Mutex<TcpListener>>,
    next_shop: TcpStream,
    prev_shop: TcpStream,
    last_token: Token,
    next_shop_id: usize,
}

impl Shop {
    pub async fn new(id: usize, total_shops: usize) -> Result<Shop, ShopError> {
        println!(
            "{}",
            format!(
                "[TIENDA {}] mi direccion para las tiendas es: {}\n",
                id,
                LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + id).to_string()
            )
            .bright_green()
        );

        println!(
            "{}",
            format!(
                "[TIENDA {}] mi direccion para los ecommerce es: {}\n",
                id,
                LOCAL_HOST.to_string() + &(PUERTO_BASE_ECOMMERCE + id).to_string()
            )
            .bright_green()
        );

        let shop_listener = Arc::new(Mutex::new(
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + id).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?,
        ));

        let ecommerce_listener = Arc::new(Mutex::new(
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_ECOMMERCE + id).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to ecommerce".to_string()))?,
        ));

        let (mut next_shop, next_shop_id) = find_next_alive(id, total_shops).await;

        if id == next_shop_id {
            let token = Token::new(0);
            next_shop
                .write_all(&token.to_bytes()?)
                .await
                .map_err(|_| ShopError::Write("Fallo en escribir el primer token".to_string()))?;
        } else {
            let syn = Syn::new(id);
            println!(
                "{}",
                format!("[TIENDA {}] Me quiero sumar al ring\n", id).bright_green()
            );
            next_shop.write_all(&syn.to_bytes()?).await.map_err(|_| {
                ShopError::Write("Fallo en escribir el syn para conectarse".to_string())
            })?;
        }

        let prev_shop = shop_listener
            .lock()
            .await
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Fallo en aceptar su anterior en el ring".to_string()))?
            .0;

        println!(
            "{}",
            format!("[TIENDA {}] Soy parte del ring\n", id).bright_green()
        );

        Ok(Shop {
            id,
            total_shops,
            shop_listener,
            ecommerce_listener,
            next_shop,
            prev_shop,
            last_token: Token::new(0),
            next_shop_id: next_id(id, total_shops),
        })
    }

    fn prev_id(&self) -> usize {
        (self.total_shops - 1 + self.id) % self.total_shops
    }

    pub async fn receive(
        &mut self,
        connection_recv: async_std::channel::Receiver<TcpStream>,
        orders_recv: Receiver<IncompleteOrder>,
        inventory: Addr<Inventory>,
    ) -> Result<(), ShopError> {
        loop {
            if let Ok(connection) = connection_recv.try_recv() {
                self.prev_shop = connection;
            }

            let mut message_type_bytes = [0u8];
            if self
                .prev_shop
                .read_exact(&mut message_type_bytes)
                .await
                .is_err()
            {
                let offline_shop = OfflineShop::new(self.prev_id(), self.id);
                if self.prev_id() == self.next_shop_id {
                    self.next_shop = TcpStream::connect(
                        LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + self.id).to_string(),
                    )
                    .await
                    .map_err(|_| {
                        ShopError::ConnectShop("Falla en conectarse a si mismo".to_string())
                    })?;
                    self.next_shop_id = self.id;
                    self.next_shop
                        .write_all(&self.last_token.to_bytes()?)
                        .await
                        .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
                } else if self
                    .next_shop
                    .write_all(&offline_shop.to_bytes()?)
                    .await
                    .is_err()
                {
                    self.next_shop = TcpStream::connect(
                        LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + self.id).to_string(),
                    )
                    .await
                    .map_err(|_| {
                        ShopError::ConnectShop("Falla en conectarse a si mismo".to_string())
                    })?;
                    self.next_shop_id = self.id;
                    self.next_shop
                        .write_all(&self.last_token.to_bytes()?)
                        .await
                        .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
                };
                self.prev_shop = connection_recv.recv().await.unwrap();
                continue;
            };
            let message_type = message_type_bytes[0];

            let mut message_len_bytes = [0u8; 8];
            self.prev_shop
                .read_exact(&mut message_len_bytes)
                .await
                .map_err(|_| {
                    ShopError::Read("Falla en leer la longitud del mensaje".to_string())
                })?;
            let message_len = usize::from_le_bytes(message_len_bytes);

            let mut bytes = vec![0u8; message_len];
            self.prev_shop
                .read_exact(&mut bytes)
                .await
                .map_err(|_| ShopError::Read("Falla en leer los bytes del mensaje".to_string()))?;

            match message_type {
                SYN_TYPE => {
                    let syn = Syn::from_bytes(&bytes)?;
                    self.handle_syn_message(syn).await?;
                }
                TOKEN_TYPE => {
                    let mut token = Token::from_bytes(&bytes)?;

                    token.increment_id();
                    self.last_token = token;
                    println!(
                        "[TIENDA {}] tengo el token, Token: {:?}\n",
                        self.id, self.last_token
                    );

                    let mut new_incomplete_orders: Vec<IncompleteOrder> = vec![];

                    for incomplete_order in self.last_token.get_incomplete_orders() {
                        if incomplete_order.get_shop_id() == self.id {
                            println!(
                                "{}",
                                format!(
                                    "[TIENDA {}] se consulto con los demas locales y ninguno puede resolver la orden: {:?}\n",
                                    self.id,
                                    Order::new(incomplete_order.get_product_id().clone(),
                                                incomplete_order.get_quantity())).yellow().bold());
                        } else {
                            inventory
                                .send(ProcessEcommerceOrder(
                                    incomplete_order.get_shop_id(),
                                    Order::new(
                                        incomplete_order.get_product_id().clone(),
                                        incomplete_order.get_quantity(),
                                    ),
                                ))
                                .await
                                .map_err(|_| {
                                    ShopError::Send(
                                        "falla enviando una orden de ecommerce al inventory"
                                            .to_string(),
                                    )
                                })?;
                        }
                    }

                    while let Ok(orden) = orders_recv.try_recv() {
                        new_incomplete_orders.push(orden)
                    }
                    self.last_token.set_incomplete_orders(new_incomplete_orders);
                    time::sleep(Duration::from_secs(1)).await;
                    self.next_shop
                        .write_all(&self.last_token.to_bytes()?)
                        .await
                        .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
                }
                OFFLINE_SHOP => {
                    let offline_shop = OfflineShop::from_bytes(&bytes)?;
                    self.handle_offline_shop_message(offline_shop).await?;
                }
                _ => {
                    println!("Mensaje no reconocido");
                }
            }
        }
    }

    pub async fn accept_shop_connection(
        &mut self,
        connection_sender: Sender<TcpStream>,
    ) -> JoinHandle<()> {
        let listener = self.shop_listener.clone();
        tokio::spawn(async move {
            loop {
                let listener = listener.lock().await;
                if let Ok(shop) = listener.accept().await {
                    if connection_sender.send(shop.0).await.is_err() {
                        println!(
                            "{:?}",
                            ShopError::Send(
                                "Falla en enviar la coneccion por el chanel".to_string()
                            )
                        );
                        return;
                    }
                } else {
                    println!(
                        "{:?}",
                        ShopError::Send("Falla en aceptar una coneccion".to_string())
                    );
                    return;
                };
            }
        })
    }

    pub async fn accept_ecommerce_connection(
        &mut self,
        inventory: Addr<Inventory>,
        handle_sender: async_std::channel::Sender<JoinHandle<()>>,
    ) -> tokio::task::JoinHandle<()> {
        let listener = self.ecommerce_listener.clone();
        let id = self.id;
        tokio::spawn(async move {
            loop {
                let listener = listener.lock().await;
                if let Ok(mut ecommerce) = listener.accept().await {
                    println!(
                        "{}",
                        format!("[TIENDA {}] nueva conexion de ecommerce aceptada\n", id)
                            .bright_green()
                    );
                    let shared_inventory = inventory.clone();
                    let handle = tokio::spawn(async move {
                        while let Ok(ecommerce_order) =
                            Order::deserialize_from_bytes(&mut ecommerce.0).await
                        {
                            if shared_inventory
                                .send(ProcessEcommerceOrder(id, ecommerce_order))
                                .await
                                .is_err()
                            {
                                println!(
                                    "{:?}",
                                    ShopError::Send(
                                        "Falla en enviar una orden de ecommerce al inventory"
                                            .to_string()
                                    )
                                );
                            };
                        }
                    });
                    handle_sender.send(handle).await.unwrap();
                };
            }
        })
    }

    async fn handle_syn_message(&mut self, syn: Syn) -> Result<(), ShopError> {
        if syn.get_id() == next_id(self.id, self.total_shops) {
            self.next_shop = TcpStream::connect(
                LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + syn.get_id()).to_string(),
            )
            .await
            .map_err(|_| ShopError::ConnectShop("Falla en conectarse al siguiente".to_string()))?;

            self.next_shop_id = syn.get_id();
            self.next_shop
                .write_all(&self.last_token.to_bytes()?)
                .await
                .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
        } else if self.next_shop.write_all(&syn.to_bytes()?).await.is_err() {
            self.next_shop = TcpStream::connect(
                LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + syn.get_id()).to_string(),
            )
            .await
            .map_err(|_| ShopError::ConnectShop("Falla en conectarse al siguiente".to_string()))?;
            self.next_shop_id = syn.get_id();
            self.next_shop
                .write_all(&self.last_token.to_bytes()?)
                .await
                .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
        };
        Ok(())
    }

    async fn handle_offline_shop_message(
        &mut self,
        offline_shop: OfflineShop,
    ) -> Result<(), ShopError> {
        if offline_shop.get_offline_id() == self.next_shop_id {
            println!(
                "{}",
                format!("[TIENDA {}] se cayo mi siguiente, lo salteo", self.id).bright_green()
            );
            self.next_shop_id = offline_shop.get_replace_id();
            self.next_shop = TcpStream::connect(
                LOCAL_HOST.to_string()
                    + &(PUERTO_BASE_SHOP + offline_shop.get_replace_id()).to_string(),
            )
            .await
            .map_err(|_| ShopError::ConnectShop("Falla en conectarse al siguiente".to_string()))?;
            self.next_shop
                .write_all(&self.last_token.to_bytes()?)
                .await
                .map_err(|_| ShopError::Write("Falla en escribir el token".to_string()))?;
        } else {
            self.next_shop
                .write_all(&offline_shop.to_bytes()?)
                .await
                .map_err(|_| ShopError::Write("Falla en escribir el offlineshop".to_string()))?;
        }
        Ok(())
    }
}

pub async fn find_next_alive(id: usize, total_shops: usize) -> (TcpStream, usize) {
    let mut i = id;
    loop {
        let next_shop_id = next_id(i, total_shops);
        if let Ok(shop) = TcpStream::connect(
            LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + next_shop_id).to_string(),
        )
        .await
        {
            return (shop, next_shop_id);
        }
        i += 1;
    }
}

fn next_id(id: usize, total_shops: usize) -> usize {
    (id + 1) % total_shops
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use actix::Actor;
    use async_std::channel::unbounded;

    use crate::{
        inventory::{SetIncompleteOrderSender, SetOrderDeliverer},
        order_deliverer::OrderDeliverer,
        shop_const::{MAX_TIME_DELIVERY, PROBABILITY_SUCCESS_DELIVERY},
    };

    use super::*;

    #[actix_rt::test]
    async fn test_syn_message_when_a_shop_is_created() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();

        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (_, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 1).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(0, 3).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Failed to accept connection".to_string()))?;

        let syn_bytes = read_channel(&mut conection).await?;
        let syn = Syn::new(0);
        assert_eq!(Syn::from_bytes(&syn_bytes)?, syn);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_without_unsolved_sales() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (_, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 3).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(2, 4).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Failed to accept connection".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 2).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        // leo el syn

        read_channel(&mut conection).await?;

        // hasta aca la inicializacion de un caso base
        let token = Token::new(1);

        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;

        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, Token::new(2));
        Ok(())
    }

    #[actix_rt::test]
    async fn test_reconection() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (_, shop_orders_recv) = mpsc::channel();
        let (connection_sender, connection_recv) = unbounded();

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 6).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(5, 7).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 5).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        // leo el syn

        read_channel(&mut conection).await?;

        previous_shop_sender
            .shutdown()
            .await
            .map_err(|_| ShopError::ConnectShop("Shutdown failed".to_string()))?;
        let offline_shop = OfflineShop::new(4, 5);
        let token_bytes = read_channel(&mut conection).await?;

        assert_eq!(OfflineShop::from_bytes(&token_bytes)?, offline_shop);

        let handler = tokio::spawn(async move { stream_2.accept().await });

        let previous_shop_sender2 =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 6).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;
        let _ = connection_sender.send(previous_shop_sender2).await;

        let mut new_conection = handler
            .await
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?
            .0;
        let token = Token::new(1);

        new_conection
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;

        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, Token::new(2));
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_with_solvable_sales() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        inventory
            .send(SetIncompleteOrderSender(shop_orders_sender))
            .await
            .map_err(|_| ShopError::Send("Failed to send IncompleteOrderSender".to_string()))?;

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 9).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(8, 10).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 8).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        // leo el syn

        let _ = read_channel(&mut conection).await;

        // hasta aca la inicializacion de un caso base

        let incomplete_order = IncompleteOrder::new(0, "product1".to_string(), 2);
        let mut token = Token::new(1);
        token.set_incomplete_orders(vec![incomplete_order]);
        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;
        let new_token = Token::new(2);
        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, new_token);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_with_solvable_and_unsolvable_sales() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        inventory
            .send(SetIncompleteOrderSender(shop_orders_sender))
            .await
            .map_err(|_| ShopError::Send("Failed to send IncompleteOrderSender".to_string()))?;

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 12).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(11, 13).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 11).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        // leo el syn

        let _ = read_channel(&mut conection).await;

        // hasta aca la inicializacion de un caso base

        let incomplete_order = IncompleteOrder::new(0, "product1".to_string(), 2);
        let incomplete_order_2 = IncompleteOrder::new(0, "product1000".to_string(), 2);
        let mut token = Token::new(1);
        token.set_incomplete_orders(vec![incomplete_order, incomplete_order_2]);
        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;
        let mut new_token = Token::new(2);
        let incomplete_order_3 = IncompleteOrder::new(0, "product1000".to_string(), 2);
        new_token.set_incomplete_orders(vec![incomplete_order_3]);
        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, new_token);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_with_unsolved_ecommerce_sale() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        inventory
            .send(SetIncompleteOrderSender(shop_orders_sender.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 15).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        let inventory_copy = inventory.clone();
        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(14, 16).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory_copy)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });

        let _send = inventory
            .send(ProcessEcommerceOrder(
                1,
                Order::new("product1000".to_string(), 2),
            ))
            .await;

        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Failed to accept connection".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 14).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        let token = Token::new(1);
        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;
        // leo el syn

        read_channel(&mut conection).await?;

        // hasta aca la inicializacion de un caso base
        let token = Token::new(1);

        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;

        let mut new_token = Token::new(2);
        let incomplete_order = IncompleteOrder::new(1, "product1000".to_string(), 2);
        new_token.set_incomplete_orders(vec![incomplete_order]);
        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, new_token);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_with_solved_ecommerce_sale() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        inventory
            .send(SetIncompleteOrderSender(shop_orders_sender.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 17).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        let inventory_copy = inventory.clone();
        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(16, 18).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory_copy)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });

        let _send = inventory
            .send(ProcessEcommerceOrder(
                1,
                Order::new("product1".to_string(), 2),
            ))
            .await;

        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Failed to accept connection".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 16).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        let token = Token::new(1);
        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;
        // leo el syn

        read_channel(&mut conection).await?;

        // hasta aca la inicializacion de un caso base
        let token = Token::new(1);

        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;

        let new_token = Token::new(2);
        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, new_token);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_token_update_with_unsolvable_sales() -> Result<(), ShopError> {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", 1);
        let inventory = Inventory::create_from_file(path_stock_inicial, 1)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .map_err(|_| ShopError::Send("Failed to send SetOrderDeliverer".to_string()))?;

        let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
        let (_, connection_recv) = unbounded();

        inventory
            .send(SetIncompleteOrderSender(shop_orders_sender))
            .await
            .map_err(|_| ShopError::Send("Failed to send IncompleteOrderSender".to_string()))?;

        let stream_2 =
            TcpListener::bind(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 20).to_string())
                .await
                .map_err(|_| ShopError::Bind("bind to shops".to_string()))?;

        tokio::spawn(async move {
            if let Ok(mut shop) = Shop::new(19, 21).await {
                if shop
                    .receive(connection_recv, shop_orders_recv, inventory)
                    .await
                    .is_err()
                {
                    assert!(false);
                };
            } else {
                assert!(false);
            };
        });
        let (mut conection, mut _adress) = stream_2
            .accept()
            .await
            .map_err(|_| ShopError::Accept("Accept failed".to_string()))?;

        let mut previous_shop_sender =
            TcpStream::connect(LOCAL_HOST.to_string() + &(PUERTO_BASE_SHOP + 19).to_string())
                .await
                .map_err(|_| {
                    ShopError::ConnectShop("Falla en conectarse al siguiente".to_string())
                })?;

        // leo el syn

        read_channel(&mut conection).await?;

        // hasta aca la inicializacion de un caso base

        let incomplete_order = IncompleteOrder::new(0, "product1000".to_string(), 2);
        let mut token = Token::new(1);
        token.set_incomplete_orders(vec![incomplete_order]);
        previous_shop_sender
            .write_all(&token.to_bytes()?)
            .await
            .map_err(|_| ShopError::Write("Fallo en escribir el token".to_string()))?;
        let mut new_token = Token::new(2);
        let new_incomplete_order = IncompleteOrder::new(0, "product1000".to_string(), 2);
        new_token.set_incomplete_orders(vec![new_incomplete_order]);
        let token_bytes = read_channel(&mut conection).await?;
        assert_eq!(Token::from_bytes(&token_bytes)?, new_token);

        Ok(())
    }

    async fn read_channel(conection: &mut TcpStream) -> Result<Vec<u8>, ShopError> {
        let mut message_type_bytes = [0u8];
        let mut message_len_bytes = [0u8; 8];

        let _type = conection
            .read_exact(&mut message_type_bytes)
            .await
            .map_err(|_| ShopError::Read("Failed to read type".to_string()))?;

        conection
            .read_exact(&mut message_len_bytes)
            .await
            .map_err(|_| ShopError::Read("Failed to read len".to_string()))?;

        let message_len = usize::from_le_bytes(message_len_bytes);
        let mut bytes = vec![0u8; message_len];

        conection
            .read_exact(&mut bytes)
            .await
            .map_err(|_| ShopError::Read("Failed to message bytes".to_string()))?;

        Ok(bytes)
    }
}
