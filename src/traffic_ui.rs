use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    symbols,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType},
    DefaultTerminal, Frame,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct TrafficApp {
    local_port: u16,
    remote_port: u16,
    upload_bytes: Arc<AtomicU64>,
    download_bytes: Arc<AtomicU64>,
    upload_data: Vec<(f64, f64)>,
    download_data: Vec<(f64, f64)>,
    window: [f64; 2],
    time_counter: f64,
    last_upload: u64,
    last_download: u64,
    max_speed: f64,
    shutdown_rx: tokio::sync::mpsc::Receiver<()>,
}

impl TrafficApp {
    pub fn new(
        local_port: u16,
        remote_port: u16,
        upload_bytes: Arc<AtomicU64>,
        download_bytes: Arc<AtomicU64>,
        shutdown_rx: tokio::sync::mpsc::Receiver<()>,
    ) -> Self {
        Self {
            local_port,
            remote_port,
            upload_bytes,
            download_bytes,
            upload_data: vec![(0.0, 0.0); 60],
            download_data: vec![(0.0, 0.0); 60],
            window: [0.0, 60.0],
            time_counter: 0.0,
            last_upload: 0,
            last_download: 0,
            max_speed: 100.0, // Start with 100 KB/s max
            shutdown_rx,
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        let tick_rate = Duration::from_millis(1000); // Update every second
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|frame| self.draw(frame))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        return Ok(());
                    }
                }
            }

            // Check for shutdown signal
            if self.shutdown_rx.try_recv().is_ok() {
                return Ok(());
            }

            if last_tick.elapsed() >= tick_rate {
                self.on_tick();
                last_tick = Instant::now();
            }
        }
    }

    fn on_tick(&mut self) {
        let current_upload = self.upload_bytes.load(Ordering::Relaxed);
        let current_download = self.download_bytes.load(Ordering::Relaxed);

        // Calculate KB/s
        let upload_kbs = (current_upload.saturating_sub(self.last_upload)) as f64 / 1024.0;
        let download_kbs = (current_download.saturating_sub(self.last_download)) as f64 / 1024.0;

        self.last_upload = current_upload;
        self.last_download = current_download;

        // Update max speed for auto-scaling
        let max_current = upload_kbs.max(download_kbs);
        if max_current > self.max_speed {
            self.max_speed = (max_current * 1.2).max(100.0); // Add 20% headroom
        }

        // Shift data
        self.time_counter += 1.0;
        self.upload_data.remove(0);
        self.upload_data.push((self.time_counter, upload_kbs));
        self.download_data.remove(0);
        self.download_data.push((self.time_counter, download_kbs));

        // Update window
        self.window[0] = self.time_counter - 59.0;
        self.window[1] = self.time_counter + 1.0;
    }

    fn draw(&self, frame: &mut Frame) {
        let areas = Layout::vertical([Constraint::Percentage(100)]).split(frame.area());

        let total_upload_mb = self.upload_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);
        let total_download_mb = self.download_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);

        let current_upload_kbs = if !self.upload_data.is_empty() {
            self.upload_data.last().unwrap().1
        } else {
            0.0
        };
        let current_download_kbs = if !self.download_data.is_empty() {
            self.download_data.last().unwrap().1
        } else {
            0.0
        };

        let title = format!(
            " TCP Relay: localhost:{} -> remote:{} | Upload: {:.2} MB ({:.1} KB/s) | Download: {:.2} MB ({:.1} KB/s) | Press 'q' to quit ",
            self.local_port,
            self.remote_port,
            total_upload_mb,
            current_upload_kbs,
            total_download_mb,
            current_download_kbs
        );

        let datasets = vec![
            Dataset::default()
                .name("Upload")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Green))
                .graph_type(GraphType::Line)
                .data(&self.upload_data),
            Dataset::default()
                .name("Download")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Cyan))
                .graph_type(GraphType::Line)
                .data(&self.download_data),
        ];

        let x_labels = vec![
            Span::styled(
                format!("{:.0}s", self.window[0].max(0.0)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{:.0}s", (self.window[0] + self.window[1]) / 2.0)),
            Span::styled(
                format!("{:.0}s", self.window[1]),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ];

        let y_max = self.max_speed;
        let y_labels = vec![
            "0".bold(),
            format!("{:.0}", y_max / 2.0).into(),
            format!("{:.0} KB/s", y_max).bold(),
        ];

        let chart = Chart::new(datasets)
            .block(
                Block::default()
                    .title(title.cyan().bold())
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            )
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .labels(x_labels)
                    .bounds(self.window),
            )
            .y_axis(
                Axis::default()
                    .title("Speed")
                    .style(Style::default().fg(Color::Gray))
                    .labels(y_labels)
                    .bounds([0.0, y_max]),
            );

        frame.render_widget(chart, areas[0]);
    }
}

pub fn run_traffic_ui(
    local_port: u16,
    remote_port: u16,
    upload_bytes: Arc<AtomicU64>,
    download_bytes: Arc<AtomicU64>,
    shutdown_rx: tokio::sync::mpsc::Receiver<()>,
) -> std::io::Result<()> {
    let terminal = ratatui::init();
    let app = TrafficApp::new(local_port, remote_port, upload_bytes, download_bytes, shutdown_rx);
    let result = app.run(terminal);
    ratatui::restore();
    result
}
