#![windows_subsystem = "windows"]

use std::sync::Arc;
use eframe::egui;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

mod binance;
use binance::BinanceClient;

enum Command {
    StartChase { side: String },
    CancelChase,
    UpdateQty(f64),
}

struct SharedState {
    connected: bool,
    bid: f64,
    ask: f64,
    qty: f64,
    chase_state_desc: String,
    logs: Vec<String>,
}

struct ChaserApp {
    shared: Arc<parking_lot::Mutex<SharedState>>,
    cmd_tx: mpsc::Sender<Command>,
    qty_input: String,
}

impl ChaserApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<parking_lot::Mutex<SharedState>>,
        cmd_tx: mpsc::Sender<Command>,
    ) -> Self {
        // Set a dark, clean visual style
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = 8.0.into();
        cc.egui_ctx.set_visuals(visuals);

        let initial_qty = shared.lock().qty;

        Self {
            shared,
            cmd_tx,
            qty_input: initial_qty.to_string(),
        }
    }

    fn add_log(&self, msg: &str) {
        let mut state = self.shared.lock();
        state.logs.push(msg.to_string());
        if state.logs.len() > 15 {
            state.logs.remove(0);
        }
    }
}

impl eframe::App for ChaserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = {
            let s = self.shared.lock();
            (s.connected, s.bid, s.ask, s.qty, s.chase_state_desc.clone(), s.logs.clone())
        };
        let (connected, bid, ask, current_qty, chase_desc, logs) = state;

        egui::CentralPanel::default().show(ctx, |ui| {
            // Header with Connection Status
            ui.horizontal(|ui| {
                ui.heading("Quick Maker Chaser");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if connected {
                        ui.colored_label(egui::Color32::from_rgb(0, 220, 100), "● WebSocket OK");
                    } else {
                        ui.colored_label(egui::Color32::from_rgb(220, 50, 50), "● Disconnected");
                    }
                });
            });

            ui.separator();

            // Book Ticker Display
            ui.columns(2, |columns| {
                columns[0].vertical_centered(|ui| {
                    ui.label("Best BUY (Bid)");
                    ui.colored_label(
                        egui::Color32::from_rgb(0, 220, 100),
                        egui::RichText::new(format!("{:.7}", bid)).size(20.0).strong(),
                    );
                });
                columns[1].vertical_centered(|ui| {
                    ui.label("Best SELL (Ask)");
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 50, 50),
                        egui::RichText::new(format!("{:.7}", ask)).size(20.0).strong(),
                    );
                });
            });

            ui.add_space(8.0);

            // Quantity Control
            ui.horizontal(|ui| {
                ui.label("Cantidad:");
                let text_edit = ui.add(egui::TextEdit::singleline(&mut self.qty_input).desired_width(100.0));
                
                if text_edit.changed() {
                    if let Ok(new_val) = self.qty_input.parse::<f64>() {
                        if new_val >= 0.0 {
                            let _ = self.cmd_tx.try_send(Command::UpdateQty(new_val));
                        }
                    }
                }

                // Preset increments
                if ui.button("+1000").clicked() {
                    let next = current_qty + 1000.0;
                    self.qty_input = next.to_string();
                    let _ = self.cmd_tx.try_send(Command::UpdateQty(next));
                }
                if ui.button("+5000").clicked() {
                    let next = current_qty + 5000.0;
                    self.qty_input = next.to_string();
                    let _ = self.cmd_tx.try_send(Command::UpdateQty(next));
                }
                if ui.button("Clear").clicked() {
                    self.qty_input = "0".to_string();
                    let _ = self.cmd_tx.try_send(Command::UpdateQty(0.0));
                }
            });

            ui.add_space(8.0);

            // Action Buttons
            ui.columns(2, |columns| {
                let buy_btn = egui::Button::new(
                    egui::RichText::new("BUY (Maker)").strong().color(egui::Color32::BLACK)
                ).fill(egui::Color32::from_rgb(0, 220, 100));

                if columns[0].add_sized([columns[0].available_width(), 40.0], buy_btn).clicked() {
                    let _ = self.cmd_tx.try_send(Command::StartChase { side: "BUY".to_string() });
                }

                let sell_btn = egui::Button::new(
                    egui::RichText::new("SELL (Maker)").strong().color(egui::Color32::WHITE)
                ).fill(egui::Color32::from_rgb(220, 50, 50));

                if columns[1].add_sized([columns[1].available_width(), 40.0], sell_btn).clicked() {
                    let _ = self.cmd_tx.try_send(Command::StartChase { side: "SELL".to_string() });
                }
            });

            ui.add_space(6.0);

            // STOP/CANCEL Button
            let stop_btn = egui::Button::new(
                egui::RichText::new("CANCEL / STOP ACTIVE CHASE").strong().color(egui::Color32::BLACK)
            ).fill(egui::Color32::from_rgb(250, 180, 20));

            if ui.add_sized([ui.available_width(), 30.0], stop_btn).clicked() {
                let _ = self.cmd_tx.try_send(Command::CancelChase);
            }

            ui.add_space(6.0);

            // Status description
            ui.horizontal(|ui| {
                ui.label("Estado:");
                ui.colored_label(egui::Color32::LIGHT_BLUE, &chase_desc);
            });

            ui.separator();

            // Logs Console
            ui.label("Historial de logs:");
            egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                for log in &logs {
                    ui.small(log);
                }
            });
        });
    }
}

#[derive(Clone, PartialEq)]
enum ChaseState {
    Idle,
    Transitioning,
    Chasing {
        side: String,
        qty: f64,
        order_id: i64,
        target_price: f64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load env variables from multiple potential locations
    let mut env_path = None;
    if std::path::Path::new("../backend/.env").exists() {
        env_path = Some("../backend/.env");
    } else if std::path::Path::new("backend/.env").exists() {
        env_path = Some("backend/.env");
    } else if std::path::Path::new(".env").exists() {
        env_path = Some(".env");
    }

    if let Some(path) = env_path {
        let _ = dotenvy::from_path(path);
    } else {
        let _ = dotenvy::dotenv();
    }

    let api_key = std::env::var("BINANCE_API_KEY")
        .unwrap_or_else(|_| "".to_string())
        .trim()
        .trim_matches('"')
        .to_string();
    let api_secret = std::env::var("BINANCE_API_SECRET")
        .unwrap_or_else(|_| "".to_string())
        .trim()
        .trim_matches('"')
        .to_string();
    let testnet = std::env::var("TESTNET")
        .unwrap_or_else(|_| "false".to_string())
        .trim()
        .parse::<bool>()
        .unwrap_or(false);

    let symbol = "1000PEPEUSDC".to_string();

    let client = BinanceClient::new(api_key.clone(), api_secret, testnet);

    let shared = Arc::new(parking_lot::Mutex::new(SharedState {
        connected: false,
        bid: 0.0,
        ask: 0.0,
        qty: 10000.0, // default starting qty
        chase_state_desc: "Idle".to_string(),
        logs: vec!["Aplicación iniciada.".to_string()],
    }));

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);

    let shared_clone = shared.clone();
    let client_clone = client.clone();
    let symbol_clone = symbol.clone();

    // Spawn Background Async Task (handling WS, REST and Chasing Logic)
    tokio::spawn(async move {
        // Output API Key Diagnostics on startup
        if api_key.is_empty() {
            log_to_shared(&shared_clone, "[ERROR] BINANCE_API_KEY no encontrada. Asegúrate de configurar backend/.env");
        } else {
            let masked = if api_key.len() > 8 {
                format!("{}...{}", &api_key[0..4], &api_key[api_key.len()-4..])
            } else {
                "Corta / Inválida".to_string()
            };
            log_to_shared(&shared_clone, &format!("[System] Archivo .env cargado con éxito. Path: {:?}", env_path));
            log_to_shared(&shared_clone, &format!("[System] API Key detectada: {}", masked));
        }

        let mut client = client_clone;
        if let Err(e) = client.sync_time().await {
            log_to_shared(&shared_clone, &format!("[Warning] Error sync time: {}", e));
        }

        let mut chase_state = ChaseState::Idle;
        let mut current_qty = 10000.0;

        // WebSocket Stream Subscription
        let ws_url = "wss://fstream.binance.com/ws/1000pepeusdc@bookTicker";
        let mut ws_connected = false;
        
        loop {
            log_to_shared(&shared_clone, "[WebSocket] Conectando...");
            match connect_async(ws_url).await {
                Ok((ws_stream, _)) => {
                    log_to_shared(&shared_clone, "[WebSocket] ¡Conectado con éxito!");
                    set_ws_connected(&shared_clone, true);
                    ws_connected = true;

                    let (_, mut ws_read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            // 1. Process Websocket Tick
                            msg = ws_read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                                            if let (Some(b_str), Some(a_str)) = (
                                                json.get("b").and_then(|v| v.as_str()),
                                                json.get("a").and_then(|v| v.as_str()),
                                            ) {
                                                let bid: f64 = b_str.parse().unwrap_or(0.0);
                                                let ask: f64 = a_str.parse().unwrap_or(0.0);

                                                update_ticker(&shared_clone, bid, ask);

                                                // Chasing Logic evaluation
                                                if let ChaseState::Chasing { side, qty, order_id, target_price } = &chase_state {
                                                    let current_maker_price = if side == "BUY" { bid } else { ask };

                                                    if (current_maker_price - *target_price).abs() > 0.0000001 {
                                                        log_to_shared(&shared_clone, &format!(
                                                            "[Chase] Precio movido a {:.7}. Re-colocando orden...",
                                                            current_maker_price
                                                        ));

                                                        let side = side.clone();
                                                        let qty = *qty;
                                                        let old_order_id = *order_id;
                                                        let symbol = symbol_clone.clone();
                                                        let client = client.clone();
                                                        let shared_bg = shared_clone.clone();

                                                        chase_state = ChaseState::Transitioning;
                                                        update_chase_desc(&shared_clone, "Re-posicionando orden...");

                                                        let (tx, mut rx) = mpsc::channel::<ChaseState>(1);

                                                        tokio::spawn(async move {
                                                            // Cancel old order
                                                            let mut cancel_success = false;
                                                            match client.cancel_order(&symbol, old_order_id).await {
                                                                Ok(_) => {
                                                                    log_to_shared(&shared_bg, &format!("[Chase] Orden {} cancelada.", old_order_id));
                                                                    cancel_success = true;
                                                                }
                                                                Err(e) => {
                                                                    let err = e.to_string();
                                                                    if err.contains("-2011") || err.contains("Order already filled") {
                                                                        log_to_shared(&shared_bg, &format!("[SUCCESS] ¡Llenada! Persecución completada (ID: {}).", old_order_id));
                                                                        let _ = tx.send(ChaseState::Idle).await;
                                                                        return;
                                                                    } else {
                                                                        log_to_shared(&shared_bg, &format!("[Warning] No se pudo cancelar {}: {}.", old_order_id, err));
                                                                        // Try placing anyway (or reset state)
                                                                        cancel_success = true;
                                                                    }
                                                                }
                                                            }

                                                            if cancel_success {
                                                                // Place new order
                                                                match client.place_limit_maker_order(&symbol, &side, qty, current_maker_price).await {
                                                                    Ok(res) => {
                                                                        if let Some(new_id) = res.get("orderId").and_then(|v| v.as_i64()) {
                                                                            log_to_shared(&shared_bg, &format!("[Chase] Nueva orden Maker colocada: ID={} Precio={:.7}", new_id, current_maker_price));
                                                                            let _ = tx.send(ChaseState::Chasing {
                                                                                side,
                                                                                qty,
                                                                                order_id: new_id,
                                                                                target_price: current_maker_price,
                                                                            }).await;
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        log_to_shared(&shared_bg, &format!("[Error] Fallo al colocar orden: {}", e));
                                                                        let _ = tx.send(ChaseState::Idle).await;
                                                                    }
                                                                }
                                                            } else {
                                                                let _ = tx.send(ChaseState::Idle).await;
                                                            }
                                                        });

                                                        // Wait for the transition to finish
                                                        if let Some(next_state) = rx.recv().await {
                                                            chase_state = next_state.clone();
                                                            match &chase_state {
                                                                ChaseState::Idle => {
                                                                    update_chase_desc(&shared_clone, "Idle");
                                                                }
                                                                ChaseState::Chasing { side, target_price, .. } => {
                                                                    update_chase_desc(&shared_clone, &format!("Perseg. {} @ {:.7}", side, target_price));
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        log_to_shared(&shared_clone, "[WebSocket] Desconectado de stream. Reintentando...");
                                        ws_connected = false;
                                        set_ws_connected(&shared_clone, false);
                                        break; // Reconnect outer loop
                                    }
                                }
                            }

                            // 2. Process Command from GUI UI thread
                            cmd = cmd_rx.recv() => {
                                if let Some(command) = cmd {
                                    match command {
                                        Command::UpdateQty(new_qty) => {
                                            current_qty = new_qty;
                                            update_shared_qty(&shared_clone, new_qty);
                                        }
                                        Command::CancelChase => {
                                            if let ChaseState::Chasing { order_id, .. } = &chase_state {
                                                let id = *order_id;
                                                let symbol = symbol_clone.clone();
                                                let client = client.clone();
                                                let shared_bg = shared_clone.clone();
                                                
                                                chase_state = ChaseState::Idle;
                                                update_chase_desc(&shared_clone, "Idle");

                                                tokio::spawn(async move {
                                                    let _ = client.cancel_order(&symbol, id).await;
                                                    log_to_shared(&shared_bg, &format!("[System] Orden ID {} cancelada.", id));
                                                });
                                            }
                                        }
                                        Command::StartChase { side } => {
                                            // Cancel any previous order if chasing
                                            if let ChaseState::Chasing { order_id, .. } = &chase_state {
                                                let id = *order_id;
                                                let symbol = symbol_clone.clone();
                                                let client = client.clone();
                                                tokio::spawn(async move {
                                                    let _ = client.cancel_order(&symbol, id).await;
                                                });
                                            }

                                            // Determine entry price from current book state
                                            let price = {
                                                let state = shared_clone.lock();
                                                if side == "BUY" { state.bid } else { state.ask }
                                            };

                                            if price == 0.0 {
                                                log_to_shared(&shared_clone, "[Error] Esperando precios de WebSocket...");
                                                continue;
                                            }

                                            log_to_shared(&shared_clone, &format!("[System] Colocando orden {} ({} contratos) a {:.7}...", side, current_qty, price));
                                            chase_state = ChaseState::Transitioning;
                                            update_chase_desc(&shared_clone, "Enviando orden inicial...");

                                            let symbol = symbol_clone.clone();
                                            let client = client.clone();
                                            let shared_bg = shared_clone.clone();
                                            let side_str = side.clone();

                                            let (tx, mut rx) = mpsc::channel::<ChaseState>(1);

                                            tokio::spawn(async move {
                                                match client.place_limit_maker_order(&symbol, &side_str, current_qty, price).await {
                                                    Ok(res) => {
                                                        if let Some(order_id) = res.get("orderId").and_then(|v| v.as_i64()) {
                                                            log_to_shared(&shared_bg, &format!("[SUCCESS] Orden ID {} colocada @ {:.7}.", order_id, price));
                                                            let _ = tx.send(ChaseState::Chasing {
                                                                side: side_str,
                                                                qty: current_qty,
                                                                order_id,
                                                                target_price: price,
                                                            }).await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        log_to_shared(&shared_bg, &format!("[Error] Error inicial: {}", e));
                                                        let _ = tx.send(ChaseState::Idle).await;
                                                    }
                                                }
                                            });

                                            if let Some(next) = rx.recv().await {
                                                chase_state = next.clone();
                                                match &chase_state {
                                                    ChaseState::Idle => {
                                                        update_chase_desc(&shared_clone, "Idle");
                                                    }
                                                    ChaseState::Chasing { side, target_price, .. } => {
                                                        update_chase_desc(&shared_clone, &format!("Perseg. {} @ {:.7}", side, target_price));
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log_to_shared(&shared_clone, &format!("[WebSocket] Error de conexión: {}. Reintentando en 3s...", e));
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    });

    // Run the native Windows GUI window using eframe
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Quick Maker Chaser")
            .with_inner_size([320.0, 310.0])
            .with_always_on_top() // Stay Floating On Top
            .with_resizable(false)
            .with_maximize_button(false),
        ..Default::default()
    };

    eframe::run_native(
        "Quick Maker Chaser",
        options,
        Box::new(|cc| Box::new(ChaserApp::new(cc, shared, cmd_tx))),
    ).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}

// Thread-safe helpers to update state and trigger GUI repaint
fn log_to_shared(shared: &Arc<parking_lot::Mutex<SharedState>>, msg: &str) {
    let mut state = shared.lock();
    state.logs.push(msg.to_string());
    if state.logs.len() > 15 {
        state.logs.remove(0);
    }
}

fn set_ws_connected(shared: &Arc<parking_lot::Mutex<SharedState>>, connected: bool) {
    shared.lock().connected = connected;
}

fn update_ticker(shared: &Arc<parking_lot::Mutex<SharedState>>, bid: f64, ask: f64) {
    let mut state = shared.lock();
    state.bid = bid;
    state.ask = ask;
}

fn update_chase_desc(shared: &Arc<parking_lot::Mutex<SharedState>>, desc: &str) {
    shared.lock().chase_state_desc = desc.to_string();
}

fn update_shared_qty(shared: &Arc<parking_lot::Mutex<SharedState>>, qty: f64) {
    shared.lock().qty = qty;
}
