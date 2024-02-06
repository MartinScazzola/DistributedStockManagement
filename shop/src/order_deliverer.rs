use actix::{Actor, Addr, Context, Handler, Message};
use rand::Rng;
use std::time::Duration;
use tokio::time;

use crate::{
    inventory::{DeliveryFailed, Inventory, SuccessfulDelivery},
    order::Order,
    shop_error::ShopError,
};

pub struct OrderDeliverer {
    inventory: Addr<Inventory>,
    succes_probability: f64,
    max_time_delivery: usize,
}

impl Actor for OrderDeliverer {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "bool")]
pub struct DeliverOrder(pub Order);

impl OrderDeliverer {
    pub fn new(
        inventory: Addr<Inventory>,
        succes_probability: f64,
        max_time_delivery: usize,
    ) -> Self {
        OrderDeliverer {
            inventory,
            succes_probability,
            max_time_delivery,
        }
    }
}

impl Handler<DeliverOrder> for OrderDeliverer {
    type Result = bool;
    fn handle(&mut self, msg: DeliverOrder, _ctx: &mut Context<Self>) -> bool {
        let order = msg.0;

        let inventory = self.inventory.clone();

        let mut rng = rand::rngs::OsRng;
        let se_entrego = rng.gen_bool(self.succes_probability);
        let max_time_delivery = self.max_time_delivery;
        tokio::spawn(async move {
            deliver_order(order, inventory, se_entrego, max_time_delivery).await
        });

        se_entrego
    }
}

async fn deliver_order(
    order: Order,
    inventory: Addr<Inventory>,
    se_entrego: bool,
    max_time_delivery: usize,
) -> bool {
    let mut rng = rand::rngs::OsRng;

    let time_to_deliver = if max_time_delivery != 0 {
        rng.gen_range(0..max_time_delivery)
    } else {
        0
    };

    println!(
        "[ORDER DELIVERER] tiempo de entrega estimado: {} segundos\n",
        time_to_deliver
    );
    time::sleep(Duration::from_secs(time_to_deliver as u64)).await;

    if se_entrego {
        println!(
            "[ORDER DELIVERER] la orden fue entregada correctamente, orden: {:?}\n",
            order
        );
        if inventory.send(SuccessfulDelivery(order)).await.is_err() {
            println!(
                "{:?}",
                ShopError::Send("Fail to send SuccessfilDelivery".to_string())
            );
        };
        true
    } else {
        println!(
            "[ORDER DELIVERER] la orden fue devuelta, orden: {:?}\n",
            order
        );
        if inventory.send(DeliveryFailed(order)).await.is_err() {
            println!(
                "{:?}",
                ShopError::Send("Fail to send DeliveryFailed".to_string())
            );
        };
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use actix::{Actor, MailboxError};

    use crate::{
        inventory::{
            AskForLockedProduct, AskForProduct, Inventory, ProcessEcommerceOrder, SetOrderDeliverer,
        },
        order::Order,
    };

    use super::*;

    #[actix_rt::test]
    async fn test_deliverer_with_0_probability_of_succes() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let inventory = Inventory::new(0, sender).start();

        let order_deliverer = OrderDeliverer::new(inventory, 0.0, 0).start();

        let se_entrego = order_deliverer
            .send(DeliverOrder(Order::new("p1".to_string(), 3)))
            .await?;

        assert!(!se_entrego);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_deliverer_with_1_probability_of_succes() -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let inventory = Inventory::new(0, sender).start();

        let order_deliverer = OrderDeliverer::new(inventory, 1.0, 0).start();

        let se_entrego = order_deliverer
            .send(DeliverOrder(Order::new("p1".to_string(), 3)))
            .await?;

        assert!(se_entrego);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_proccess_ecommerce_order_with_deliverer_0_probability_of_succes(
    ) -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);

        inventory.insert_item("p1".to_string(), 15);

        let inventory = inventory.start();

        let order_deliverer = OrderDeliverer::new(inventory.clone(), 0.0, 0).start();

        inventory.send(SetOrderDeliverer(order_deliverer)).await?;

        inventory
            .send(ProcessEcommerceOrder(0, Order::new("p1".to_string(), 3)))
            .await?;

        time::sleep(Duration::from_millis(1 as u64)).await;

        let quantity_items = inventory.send(AskForProduct("p1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("p1".to_string()))
            .await?;

        assert_eq!(quantity_items, 15);
        assert_eq!(quantity_locked_items, 0);
        Ok(())
    }

    #[actix_rt::test]
    async fn test_proccess_ecommerce_order_with_deliverer_1_probability_of_succes(
    ) -> Result<(), MailboxError> {
        // Crear el inventario con algunos elementos de prueba
        let (sender, _) = mpsc::channel();
        let mut inventory = Inventory::new(0, sender);

        inventory.insert_item("p1".to_string(), 15);

        let inventory = inventory.start();

        let order_deliverer = OrderDeliverer::new(inventory.clone(), 1.0, 1).start();

        inventory.send(SetOrderDeliverer(order_deliverer)).await?;

        inventory
            .send(ProcessEcommerceOrder(0, Order::new("p1".to_string(), 3)))
            .await?;

        let quantity_items = inventory.send(AskForProduct("p1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("p1".to_string()))
            .await?;

        assert_eq!(quantity_items, 12);
        assert_eq!(quantity_locked_items, 3);

        time::sleep(Duration::from_millis(1001 as u64)).await;

        let quantity_items = inventory.send(AskForProduct("p1".to_string())).await?;
        let quantity_locked_items = inventory
            .send(AskForLockedProduct("p1".to_string()))
            .await?;

        assert_eq!(quantity_items, 12);
        assert_eq!(quantity_locked_items, 0);
        Ok(())
    }
}
