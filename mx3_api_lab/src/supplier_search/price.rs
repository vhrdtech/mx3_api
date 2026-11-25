//! The price of a quantity at a supplier (SRCH-4): every packaging of a product is an [Offer] with its own price
//! breaks, minimum order, order step and stock (Digi-Key's cut tape, Digi-Reel and tape & reel of one MPN). Below
//! the minimum order or between steps one buys more than needed, and that is what the quantity costs; the cheapest
//! offer per piece needed wins. Pure.

use serde::{Deserialize, Serialize};

/// One way a supplier sells a product: a packaging with its price breaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    /// The supplier's part number of this packaging (`490-13342-1-ND`)
    pub spn: String,
    /// As the supplier names it: `Cut Tape (CT)`, `Tape & Reel (TR)`, `Digi-Reel®`, `Bulk`...
    pub packaging: String,
    /// (break quantity, unit price in USD), any order
    pub breaks: Vec<(u32, f64)>,
    /// The fewest one can order (1 when the supplier doesn't say)
    pub min_order: u32,
    /// Orders go in steps of this many (LCSC's increment; 1 when the supplier doesn't say)
    #[serde(default = "one")]
    pub order_multiple: u32,
    /// In stock for this packaging
    pub stock: u64,
    /// A fee per order line on top (Digi-Reel's reeling fee)
    pub fee: f64,
    /// Sold by a third party through the supplier's marketplace
    pub marketplace: bool,
}

fn one() -> u32 {
    1
}

/// What `qty` costs with one offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QtyPrice {
    pub spn: String,
    pub packaging: String,
    /// What one has to buy for `qty`: at least the minimum order, in whole order steps
    pub bought: u32,
    /// The unit price of the break `bought` falls in
    pub unit_price: f64,
    /// `bought` × unit price + fee
    pub extended: f64,
    /// `extended / qty`: what one needed piece really costs, the fee and the surplus spread
    pub effective_unit: f64,
    /// The break quantity the unit price comes from
    pub break_qty: u32,
    pub stock: u64,
    /// The offer has `bought` in stock
    pub in_stock: bool,
}

impl Offer {
    /// The price of `qty` pieces, `None` without a price break at or below what has to be bought.
    pub fn price_at(&self, qty: u32) -> Option<QtyPrice> {
        let qty = qty.max(1);
        let step = self.order_multiple.max(1);
        let bought = qty.max(self.min_order).div_ceil(step).saturating_mul(step);
        let (break_qty, unit_price) = self
            .breaks
            .iter()
            .filter(|(b, p)| *b <= bought && p.is_finite())
            .max_by_key(|(b, _)| *b)
            .copied()?;
        let extended = unit_price * f64::from(bought) + self.fee;
        Some(QtyPrice {
            spn: self.spn.clone(),
            packaging: self.packaging.clone(),
            bought,
            unit_price,
            extended,
            effective_unit: extended / f64::from(qty),
            break_qty,
            stock: self.stock,
            in_stock: self.stock >= u64::from(bought),
        })
    }
}

/// The cheapest way to buy `qty` among `offers` (marketplace ones only with `marketplace`), by what one piece really
/// costs; offers with the quantity in stock first when `in_stock`. `None` when no offer sells that quantity.
pub fn best_price(
    offers: &[Offer],
    qty: u32,
    marketplace: bool,
    in_stock: bool,
) -> Option<QtyPrice> {
    offers
        .iter()
        .filter(|o| marketplace || !o.marketplace)
        .filter_map(|o| o.price_at(qty))
        .min_by(|a, b| {
            let stock = |p: &QtyPrice| in_stock && !p.in_stock;
            stock(a)
                .cmp(&stock(b))
                .then(a.effective_unit.total_cmp(&b.effective_unit))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GRM155R71H104KE14J as Digi-Key sells it (digi_key_api/assets/digikey_product_details_v4.json)
    fn murata() -> Vec<Offer> {
        let ct = vec![
            (1, 0.1),
            (10, 0.025),
            (100, 0.0139),
            (1000, 0.00852),
            (2500, 0.0072),
            (5000, 0.0064),
        ];
        vec![
            Offer {
                spn: "490-13342-2-ND".into(),
                packaging: "Tape & Reel (TR)".into(),
                breaks: vec![(50000, 0.00489), (100000, 0.00453)],
                min_order: 50000,
                order_multiple: 1,
                stock: 950000,
                fee: 0.0,
                marketplace: false,
            },
            Offer {
                spn: "490-13342-1-ND".into(),
                packaging: "Cut Tape (CT)".into(),
                breaks: ct.clone(),
                min_order: 1,
                order_multiple: 1,
                stock: 954724,
                fee: 0.0,
                marketplace: false,
            },
            Offer {
                spn: "490-13342-6-ND".into(),
                packaging: "Digi-Reel®".into(),
                breaks: ct,
                min_order: 1,
                order_multiple: 1,
                stock: 954724,
                fee: 7.0,
                marketplace: false,
            },
        ]
    }

    #[test]
    fn cut_tape_at_25() {
        let p = best_price(&murata(), 25, false, true).unwrap();
        assert_eq!(p.spn, "490-13342-1-ND");
        assert_eq!(p.unit_price, 0.025);
        assert_eq!(p.break_qty, 10);
        assert_eq!(p.bought, 25);
        assert!((p.extended - 0.625).abs() < 1e-9);
    }

    #[test]
    fn reel_fee_spread_and_minimum_order() {
        // at 1000 cut tape and Digi-Reel share the price, the reeling fee makes Digi-Reel dearer
        let p = best_price(&murata(), 1000, false, true).unwrap();
        assert_eq!(p.packaging, "Cut Tape (CT)");
        // a full reel only from its minimum order: 1000 pieces cost a whole reel, so cut tape wins
        let reel = murata()[0].price_at(1000).unwrap();
        assert_eq!(reel.bought, 50000);
        assert!(reel.effective_unit > 0.2);
        let p = best_price(&murata(), 50000, false, true).unwrap();
        assert_eq!(p.packaging, "Tape & Reel (TR)");
        assert_eq!(p.unit_price, 0.00489);
    }

    #[test]
    fn minimum_order_and_steps() {
        let o = Offer {
            spn: "C1525".into(),
            packaging: "Cut Tape".into(),
            breaks: vec![(10, 0.002), (100, 0.0015)],
            min_order: 10,
            order_multiple: 10,
            stock: 1000,
            fee: 0.0,
            marketplace: false,
        };
        let p = o.price_at(5).unwrap();
        assert_eq!((p.bought, p.unit_price), (10, 0.002));
        assert!((p.effective_unit - 0.004).abs() < 1e-12);
        let p = o.price_at(95).unwrap();
        // 95 rounds up to 100, into the next break
        assert_eq!((p.bought, p.unit_price), (100, 0.0015));
        let none = Offer {
            breaks: vec![(1000, 0.1)],
            min_order: 1,
            ..o
        };
        assert!(none.price_at(5).is_none());
    }

    #[test]
    fn stock_and_marketplace() {
        let mut offers = murata();
        offers[1].stock = 10;
        offers[2].stock = 10;
        offers.push(Offer {
            spn: "MP-1".into(),
            packaging: "Bulk".into(),
            breaks: vec![(1, 0.001)],
            min_order: 1,
            order_multiple: 1,
            stock: 1_000_000,
            fee: 0.0,
            marketplace: true,
        });
        // marketplace left out. Cut tape lacks the stock: in stock now only as a whole reel, else cut tape (flagged)
        let p = best_price(&offers, 25, false, true).unwrap();
        assert_eq!((p.spn.as_str(), p.bought), ("490-13342-2-ND", 50000));
        let p = best_price(&offers, 25, false, false).unwrap();
        assert_eq!(p.spn, "490-13342-1-ND");
        assert!(!p.in_stock);
        let p = best_price(&offers, 25, true, true).unwrap();
        assert_eq!(p.spn, "MP-1");
        assert!(best_price(&[], 1, true, false).is_none());
    }
}
