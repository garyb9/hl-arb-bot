//! Engine risk-gate benchmark (SPEC-0010 §17, PERF-014 follow-up).
//!
//! Measures [`RiskGate::evaluate`] for one place when 50 working orders rest
//! across 20 coins and the target coin is already at its open-order cap, so the
//! call reaches (and is rejected by) the per-coin open-order check. That check
//! must stay O(1): the benchmark is flat as the resting book grows, rather than
//! scaling with the total number of tracked orders.

use std::str::FromStr;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rust_decimal::Decimal;

use hl_arb_engine::risk::{Breakers, RateBudget, RateBudgetConfig, RiskCtx, RiskGate, RiskLimits};
use hl_arb_engine::{
    AccountState, Action, AssetMeta, Cloid, CoinId, Level, LiveOrder, MarketSlot, OrderManager,
    OrderState, Side, Stamp,
};
use hl_arb_risk::kill::KillSwitch;
use hl_arb_strategy::{OrderIntent, Side as StrategySide, StrategyId, TimeInForce};

const COINS: usize = 20;
const ORDERS: usize = 50;
const CAP: usize = 3;

fn ds(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn level(px: i64) -> Level {
    Level {
        px: Decimal::from(px),
        sz: Decimal::ONE,
        n: 1,
    }
}

fn cloid(n: u32) -> Cloid {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&n.to_le_bytes());
    Cloid(bytes)
}

fn live(cloid: Cloid, coin: u16) -> LiveOrder {
    LiveOrder {
        cloid,
        coin: CoinId(coin),
        side: Side::Buy,
        px: ds("100"),
        sz: Decimal::ONE,
        filled_sz: Decimal::ZERO,
        reduce_only: false,
        strategy: StrategyId::from("bench"),
        state: OrderState::Resting,
        req_id: None,
        oid: None,
    }
}

fn place() -> Action {
    Action::Place(OrderIntent {
        strategy: StrategyId::from("bench"),
        coin: "BTC".into(),
        side: StrategySide::Buy,
        limit_px: Some(ds("100")),
        size: Decimal::ONE,
        tif: TimeInForce::Alo,
        reduce_only: false,
        rationale: "bench".into(),
        cloid: None,
        signal_ms: 0,
        decision_ms: 0,
    })
}

fn risk_gate(c: &mut Criterion) {
    // 50 resting orders over 20 coins; coin 0 has CAP of them, so the place
    // below is refused by the open-order check.
    let mut orders = OrderManager::new(COINS);
    for i in 0..ORDERS {
        let coin = if i < CAP {
            0
        } else {
            1 + (i - CAP) % (COINS - 1)
        };
        orders.insert(live(cloid(i as u32 + 1), coin as u16));
    }
    let account = AccountState::new(COINS);
    let slot = MarketSlot {
        bbo: Some((Some(level(99)), Some(level(101)), Stamp::default())),
        ..Default::default()
    };
    let meta = AssetMeta {
        asset_id: 0,
        sz_decimals: 0,
        is_spot: false,
        tick_size: None,
    };
    let budget = RateBudget::new(RateBudgetConfig {
        ip_per_min: 1_000_000,
        address_per_min: 1_000_000,
        ip_min: 0,
        address_min: 0,
        hard_floor: 0,
    });
    let mut gate = RiskGate::new(
        RiskLimits {
            max_order_notional: Some(ds("5000")),
            max_position_notional: Some(ds("100000")),
            max_margin_utilization_bps: Some(ds("9000")),
            max_open_orders: Some(CAP),
            min_notional: ds("10"),
        },
        KillSwitch::new(),
        Breakers::new(),
    );
    let action = place();

    c.bench_function("risk/open_order_cap_50_orders_20_coins", |b| {
        b.iter(|| {
            let ctx = RiskCtx {
                coin: CoinId(0),
                orders: &orders,
                account: &account,
                slot: &slot,
                meta: &meta,
                rate: &budget,
                now_ms: 0,
            };
            let _ = black_box(gate.evaluate(black_box(&action), &ctx));
        })
    });
}

criterion_group!(benches, risk_gate);
criterion_main!(benches);
