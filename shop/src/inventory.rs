use std::{
    cmp::Ordering,
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    sync::mpsc::Sender,
};

extern crate colored;

use colored::*;

use actix::{Actor, Addr, Context, Handler, Message};

use crate::{
    incomplete_order::IncompleteOrder,
    order::Order,
    order_deliverer::{DeliverOrder, OrderDeliverer},
    shop_error::ShopError,
};

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProcessShopOrder(pub Order);

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProcessEcommerceOrder(pub usize, pub Order);

#[derive(Message)]
#[rtype(result = "()")]
pub struct SuccessfulDelivery(pub Order);

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetOrderDeliverer(pub Addr<OrderDeliverer>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct SetIncompleteOrderSender(pub Sender<IncompleteOrder>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct DeliveryFailed(pub Order);

#[derive(Message)]
#[rtype(result = "usize")]
pub struct AskForProduct(pub String);

#[derive(Message)]
#[rtype(result = "usize")]
pub struct AskForLockedProduct(pub String);

#[derive(Debug)]
pub struct Inventory {
    items: HashMap<String, usize>,
    locked_items: HashMap<String, usize>,
    incomplete_order_sender: Option<Sender<IncompleteOrder>>,
    shop_id: usize,
    order_deliverer: Option<Addr<OrderDeliverer>>,
}

impl Actor for Inventory {
    type Context = Context<Self>;
}

impl Inventory {
    pub fn new(shop_id: usize, incomplete_order_sender: Sender<IncompleteOrder>) -> Self {
        let items = HashMap::new();
        let locked_items = HashMap::new();
        Inventory {
            items,
            locked_items,
            incomplete_order_sender: Some(incomplete_order_sender),
            shop_id,
            order_deliverer: None,
        }
    }

    pub fn create_from_file(path: String, shop_id: usize) -> Result<Self, ShopError> {
        let mut items = HashMap::new();
        let file = File::open(path).map_err(|_| {
            ShopError::OpenFile(
                "Fallo en abrir el archivo del stock inicial del inventario".to_string(),
            )
        })?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let fields: Vec<&str> = line.split(": ").collect();
                    let product_id = fields[0].to_string();
                    if let Ok(cant) = fields[1].parse() {
                        items.insert(product_id, cant);
                    } else {
                        continue;
                    };
                }
                Err(_) => {
                    continue;
                }
            }
        }

        Ok(Inventory {
            items,
            locked_items: HashMap::new(),
            incomplete_order_sender: None,
            shop_id,
            order_deliverer: None,
        })
    }

    pub fn insert_item(&mut self, product: String, quantity: usize) {
        self.items.insert(product, quantity);
    }

    pub fn deduct_product(&mut self, item: &String, quantity: usize) -> Result<(), ShopError> {
        if let Some(entry) = self.items.get_mut(item) {
            match (*entry).cmp(&quantity) {
                Ordering::Greater => {
                    *entry -= quantity;
                }
                Ordering::Equal => {
                    self.items.remove(item);
                }
                Ordering::Less => {
                    return Err(ShopError::InsufficientStock(
                        "Insufficient stock".to_string(),
                    ));
                }
            }
        } else {
            return Err(ShopError::InsufficientStock(
                "Insufficient stock".to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_order_deliverer(&mut self, order_deliverer_actor: Addr<OrderDeliverer>) {
        self.order_deliverer = Some(order_deliverer_actor);
    }

    pub fn deduct_lock_product(&mut self, item: &String, quantity: usize) -> Result<(), ShopError> {
        if let Some(entry) = self.locked_items.get_mut(item) {
            match (*entry).cmp(&quantity) {
                Ordering::Greater => {
                    *entry -= quantity;
                }
                Ordering::Equal => {
                    self.locked_items.remove(item);
                }
                Ordering::Less => {
                    return Err(ShopError::InsufficientStock(
                        "Se reservo un producto y ahora no esta".to_string(),
                    ));
                }
            }
        } else {
            return Err(ShopError::InsufficientStock(
                "Se reservo un producto y ahora no esta".to_string(),
            ));
        }
        Ok(())
    }

    pub fn lock_product(&mut self, item: &String, quantity: usize) -> Result<(), ShopError> {
        if let Some(entry) = self.items.get_mut(item) {
            match (*entry).cmp(&quantity) {
                Ordering::Greater => {
                    *entry -= quantity;
                }
                Ordering::Equal => {
                    self.items.remove(item);
                }
                Ordering::Less => {
                    return Err(ShopError::InsufficientStock(
                        "Insufficient stock".to_string(),
                    ));
                }
            }
        } else {
            return Err(ShopError::InsufficientStock(
                "Insufficient stock".to_string(),
            ));
        }
        if let Some(entry) = self.locked_items.get_mut(item) {
            *entry += quantity
        } else {
            self.locked_items.insert(item.clone(), quantity);
        };

        Ok(())
    }

    pub fn unlock_product(&mut self, item: &String, quantity: usize) -> Result<(), ShopError> {
        if let Some(entry) = self.locked_items.get_mut(item) {
            match (*entry).cmp(&quantity) {
                Ordering::Greater => {
                    *entry -= quantity;
                }
                Ordering::Equal => {
                    self.locked_items.remove(item);
                }
                Ordering::Less => {
                    return Err(ShopError::InsufficientStock(
                        "Insufficient stock".to_string(),
                    ));
                }
            }
        } else {
            return Err(ShopError::InsufficientStock(
                "Insufficient stock".to_string(),
            ));
        }
        if let Some(entry) = self.items.get_mut(item) {
            *entry += quantity
        } else {
            self.items.insert(item.clone(), quantity);
        };

        Ok(())
    }
}

impl Handler<ProcessShopOrder> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: ProcessShopOrder, _ctx: &mut Context<Self>) -> Self::Result {
        let order = msg.0;

        let product_id = order.get_product_id();
        let quantity = order.get_quantity();

        if self.deduct_product(&product_id, quantity).is_err() {
            println!(
                "{}",
                format!(
                    "[INVENTARIO {}] stock insuficiente orden de tienda fisica, no se puede resolver {:?}\n",
                    self.shop_id,
                    order
                ).bright_cyan()
            );
        } else {
            println!(
                "{}",
                format!(
                    "[INVENTARIO {}] tengo producto para resolver la ordende tienda fisica: {:?}\n",
                    self.shop_id, order
                )
                .bright_cyan()
            );
        };
    }
}

impl Handler<ProcessEcommerceOrder> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: ProcessEcommerceOrder, _ctx: &mut Context<Self>) -> Self::Result {
        let order = msg.1;

        let product_id = order.get_product_id();
        let quantity = order.get_quantity();

        if self.lock_product(&product_id, quantity).is_err() {
            println!(
                "{}",
                format!(
                    "[INVENTARIO {}] stock insuficiente para resolver la orden de ecommerce, se consulta a otra tienda\n",
                    self.shop_id
                ).yellow().bold()
            );
            if self
                .incomplete_order_sender
                .as_ref()
                .unwrap()
                .send(IncompleteOrder::new(msg.0, product_id, quantity))
                .is_err()
            {
                println!(
                    "{}",
                    format!(
                        "[INVENTARIO {}] se perdio la orden {:?}",
                        self.shop_id, order
                    )
                    .yellow()
                    .bold()
                );
            };
        } else {
            println!(
                "{}",
                format!(
                    "[INVENTARIO {}] tengo producto para resolver la orden de ecommerce de la tienda origen [TIENDA {}], la envio al order deliverer: {:?}\n",
                    self.shop_id, msg.0, order
                ).yellow().bold()
            );
            if let Some(deliverer) = &self.order_deliverer {
                deliverer.do_send(DeliverOrder(order));
            } else {
                println!("No hay order deliverer");
            }
        };
    }
}

impl Handler<SuccessfulDelivery> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: SuccessfulDelivery, _ctx: &mut Context<Self>) -> Self::Result {
        let order = msg.0;

        let product_id = order.get_product_id();
        let quantity = order.get_quantity();

        if let Err(err) = self.deduct_lock_product(&product_id, quantity) {
            println!("{:?}", err);
        };
    }
}

impl Handler<DeliveryFailed> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: DeliveryFailed, _ctx: &mut Context<Self>) -> Self::Result {
        let order = msg.0;

        let product_id = order.get_product_id();
        let quantity = order.get_quantity();

        if let Err(err) = self.unlock_product(&product_id, quantity) {
            println!("{:?}", err);
        };
    }
}

impl Handler<SetOrderDeliverer> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: SetOrderDeliverer, _ctx: &mut Context<Self>) -> Self::Result {
        let order_deliverer_actor = msg.0;
        self.set_order_deliverer(order_deliverer_actor);
    }
}

impl Handler<SetIncompleteOrderSender> for Inventory {
    type Result = ();

    fn handle(&mut self, msg: SetIncompleteOrderSender, _ctx: &mut Context<Self>) -> Self::Result {
        let incomplete_order_sender = msg.0;
        self.incomplete_order_sender = Some(incomplete_order_sender);
    }
}

impl Handler<AskForProduct> for Inventory {
    type Result = usize;

    fn handle(&mut self, msg: AskForProduct, _ctx: &mut Context<Self>) -> usize {
        let product = msg.0;
        match self.items.get(&product) {
            Some(quantity) => *quantity,
            None => 0,
        }
    }
}

impl Handler<AskForLockedProduct> for Inventory {
    type Result = usize;

    fn handle(&mut self, msg: AskForLockedProduct, _ctx: &mut Context<Self>) -> usize {
        let product = msg.0;
        match self.locked_items.get(&product) {
            Some(quantity) => *quantity,
            None => 0,
        }
    }
}

impl std::fmt::Display for Inventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Items")?;
        for (p, q) in self.items.iter() {
            writeln!(f, "{} {}", p, q)?;
        }
        writeln!(f, "Locked items")?;
        for (p, q) in self.locked_items.iter() {
            writeln!(f, "{} {}", p, q)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use actix::MailboxError;

    use super::*;
    use std::sync::mpsc;

    #[actix_rt::test]
    async fn test_process_shop_order() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();

        let mut inventory = Inventory::new(0, sender);
        inventory.items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(ProcessShopOrder(Order::new("item1".to_string(), 3)))
            .await?;

        let quantity = inventory.send(AskForProduct("item1".to_string())).await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity, 7);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_process_shop_order_invalid_quantity() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(ProcessShopOrder(Order::new("item1".to_string(), 15)))
            .await?;

        let quantity = inventory.send(AskForProduct("item1".to_string())).await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity, 10);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_process_ecommerce_order_valid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(ProcessEcommerceOrder(0, Order::new("item1".to_string(), 3)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 7);
        assert_eq!(quantity_locked_items, 3);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_process_ecommerce_order_invalid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(ProcessEcommerceOrder(
                0,
                Order::new("item1".to_string(), 15),
            ))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 10);
        assert_eq!(quantity_locked_items, 0);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_succesfull_delivery_valid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.locked_items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(SuccessfulDelivery(Order::new("item1".to_string(), 5)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 0);
        assert_eq!(quantity_locked_items, 5);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_succesfull_delivery_invalid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.locked_items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(SuccessfulDelivery(Order::new("item1".to_string(), 15)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 0);
        assert_eq!(quantity_locked_items, 10);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_delivery_failed_valid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.locked_items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(DeliveryFailed(Order::new("item1".to_string(), 5)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 5);
        assert_eq!(quantity_locked_items, 5);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_delivery_failed_invalid_product() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);
        inventory.locked_items.insert("item1".to_string(), 10);

        // Iniciar el actor
        let inventory = inventory.start();

        // Enviar un mensaje al actor para procesar una orden de tienda física
        inventory
            .send(DeliveryFailed(Order::new("item1".to_string(), 15)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("item1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("item1".to_string()))
            .await?;

        // Verificar que la cantidad de "item1" se ha reducido correctamente
        assert_eq!(quantity_items, 0);
        assert_eq!(quantity_locked_items, 10);
        Ok(())
    }
}
