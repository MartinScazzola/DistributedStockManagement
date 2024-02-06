use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, Read},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

extern crate colored;

use colored::*;

use async_std::channel::unbounded;
use tokio::time::{self, Duration};

use actix::{Actor, Addr, System};
use shop::{
    inventory::{Inventory, ProcessShopOrder, SetIncompleteOrderSender, SetOrderDeliverer},
    order::Order,
    order_deliverer::OrderDeliverer,
    shop::Shop,
    shop_const::{CONNECT, DISCONNECT, MAX_TIME_DELIVERY, PROBABILITY_SUCCESS_DELIVERY},
    shop_error::ShopError,
};

fn args_parser() -> Result<(usize, usize), ShopError> {
    let args: Vec<String> = env::args().collect();

    let shop_id: usize = args[1]
        .parse()
        .map_err(|_| ShopError::ParseArgs("Fallo parseando el shop id".to_string()))?;
    let total_shops: usize = args[2]
        .parse()
        .map_err(|_| ShopError::ParseArgs("Fallo parseando el total shops".to_string()))?;

    Ok((shop_id, total_shops))
}

fn main() -> Result<(), ShopError> {
    let (shop_id, total_shops) = args_parser()?;
    let (action_sender, action_receiver): (Sender<u8>, Receiver<u8>) = mpsc::channel();

    read_stdin(action_sender);

    let system = System::new();
    system.block_on(async {
        let path_stock_inicial = format!("../orders/stock_inicial_{}.txt", shop_id);
        let inventory = Inventory::create_from_file(path_stock_inicial, shop_id)?.start();
        let order_deliverer = OrderDeliverer::new(
            inventory.clone(),
            PROBABILITY_SUCCESS_DELIVERY,
            MAX_TIME_DELIVERY,
        )
        .start();
        if inventory
            .send(SetOrderDeliverer(order_deliverer.clone()))
            .await
            .is_err()
        {
            return Err(ShopError::Send(
                "Fallo en setear la direccion del delivery al inventario".to_string(),
            ));
        };

        let shared_inventory = inventory.clone();
        tokio::spawn(async move {
            let path_orders_shop = format!("../orders/orders_local_{}.txt", shop_id);
            make_shop_orders(path_orders_shop, shared_inventory)
                .await
                .unwrap();
        });

        loop {
            let (shop_orders_sender, shop_orders_recv) = mpsc::channel();
            inventory
                .send(SetIncompleteOrderSender(shop_orders_sender))
                .await
                .unwrap();

            let (connection_sender, connection_recv) = unbounded();
            let (handle_sender, handle_recv) = unbounded();
            let shared_inventory = inventory.clone();

            let mut shop = Shop::new(shop_id, total_shops).await?;

            let mut shop_handles = vec![];

            let handle_accept_shop = shop.accept_shop_connection(connection_sender).await;
            shop_handles.push(handle_accept_shop);

            let handle_accept_ecommerce = shop
                .accept_ecommerce_connection(inventory.clone(), handle_sender)
                .await;
            shop_handles.push(handle_accept_ecommerce);

            let handle_receive = tokio::spawn(async move {
                if let Err(err) = shop
                    .receive(connection_recv, shop_orders_recv, shared_inventory)
                    .await
                {
                    println!("{:?}", err);
                };
            });
            shop_handles.push(handle_receive);

            loop {
                if let Ok(action) = action_receiver.try_recv() {
                    if action == DISCONNECT {
                        for handle in shop_handles {
                            handle.abort();
                        }

                        while let Ok(handler) = handle_recv.try_recv() {
                            handler.abort();
                        }
                        break;
                    }
                }
                time::sleep(Duration::from_secs(1)).await;
            }

            println!(
                "{}",
                format!("[TIENDA {}] me desconecto \n", shop_id,).bright_red(),
            );
            wait_for_connect_action(&action_receiver).await;
        }
    })?;

    system
        .run()
        .map_err(|_| ShopError::SystemRun("Falla en system run".to_string()))?;
    Ok(())
}

async fn make_shop_orders(path: String, inventory: Addr<Inventory>) -> Result<(), ShopError> {
    let file = File::open(path).map_err(|_| {
        ShopError::OpenFile("Fallo abriendo el archivo de ordenes de tienda fisica".to_string())
    })?;
    let reader = BufReader::new(file);

    for line in reader.lines().flatten() {
        if let Ok(order) = Order::parse_order(line) {
            if inventory.send(ProcessShopOrder(order)).await.is_err() {
                println!(
                    "{:?}",
                    ShopError::Send("Fail to send shop order to inventory".to_string())
                )
            };
        }
        time::sleep(Duration::from_secs(3)).await;
    }
    Ok(())
}

fn read_stdin(action_sender: Sender<u8>) {
    thread::spawn(move || loop {
        let mut buf = [0u8];
        io::stdin().read_exact(&mut buf).unwrap();
        action_sender.send(buf[0]).unwrap();
    });
}

async fn wait_for_connect_action(action_receiver: &Receiver<u8>) {
    loop {
        if let Ok(action) = action_receiver.try_recv() {
            if action == CONNECT {
                break;
            }
        }
        time::sleep(Duration::from_secs(1)).await;
    }
}
