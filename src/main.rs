use eframe::egui;
use egui_plot::PlotPoint;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use hound;
use rustfft::{num_complex::Complex, FftPlanner};
use std::fs::File;
use std::io::Write;
use std::ops::AddAssign;
use std::path::Path;

fn read_wav(file_path: &str) -> Result<(Vec<f32>, u32), String> {
    let reader = hound::WavReader::open(file_path).map_err(|e| e.to_string())?;
    let sample_rate = reader.spec().sample_rate;
    let samples: Vec<f32> = reader
        .into_samples::<i16>()
        .filter_map(Result::ok)
        .map(|s| s as f32)
        .collect();
    Ok((samples, sample_rate))
}

fn read_f(file_path: &str) -> Result<(Vec<f32>, Vec<f32>), String> {
    let file = File::open(file_path).map_err(|e| e.to_string())?;
    let plot_data: PlotData = bincode::deserialize_from(file).map_err(|e| e.to_string())?;
    Ok((plot_data.freqs, plot_data.amplitudes))
}

fn fourier_analysis(samples: &[f32], sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(samples.len());
    let mut buffer: Vec<Complex<f32>> = samples.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buffer);

    let freqs: Vec<f32> = (0..buffer.len() / 2)
        .map(|i| i as f32 * sample_rate as f32 / samples.len() as f32)
        .collect();
    let amplitudes: Vec<f32> = buffer.iter().take(buffer.len() / 2).map(|c| c.norm()).collect();
    (freqs, amplitudes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <folder>", args[0]);
        return Ok(());
    }
    let folder_path = &args[1];
    let all_files = std::fs::read_dir(folder_path)?;
    let mut plots = Vec::new();
    for file in all_files {
        let file_path = file?.path().display().to_string();
        let (freqs, amplitudes) = if file_path.ends_with(".wav") {
            let t0 = std::time::Instant::now();
            let (samples, sample_rate) = read_wav(&file_path)?;
            let (freqs, amplitudes) = fourier_analysis(&samples, sample_rate);
            println!("Time taken for reading wav: {:?}", t0.elapsed());
            (freqs, amplitudes)
            // } else if file_path.ends_with(".mp3") {
            //     read_mp3(file_path)?
        } else if file_path.ends_with(".f") {
            let t0 = std::time::Instant::now();
            let res = read_f(&file_path)?;
            println!("Time taken for reading f: {:?}", t0.elapsed());
            res
        } else {
            eprintln!("Unsupported file format");
            return Ok(());
        };

        plots.push(PlotData {
            freqs,
            amplitudes,
            file_name: file_path.to_string(),
        });
    }

    println!("Starting eframe with {} plots", plots.len());
    match eframe::run_native(
        "Frequency Spectrum",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(MyApp::new(cc, plots)))),
    ) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PlotData {
    freqs: Vec<f32>,
    amplitudes: Vec<f32>,
    file_name: String,
}

impl Default for PlotData {
    fn default() -> Self {
        Self {
            freqs: vec![],
            amplitudes: vec![],
            file_name: "".to_string(),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct MyApp {
    #[serde(skip)]
    plots: Vec<PlotData>,
    #[serde(skip)]
    avg_plot: PlotData,
    min_freq: f32,
    max_freq: f32,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            plots: Vec::new(),
            avg_plot: PlotData::default(),
            min_freq: 0.0,
            max_freq: 20_000.0,
        }
    }
}

impl MyApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>, plots: Vec<PlotData>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        let mut app:Self = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Self::default()
        };

        app.plots = plots;
        app
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.button("Save average plot").clicked() {
                let encoded: Vec<u8> = bincode::serialize(&self.avg_plot).unwrap();
                let path = Path::new("average_plot.f");
                let mut file = File::create(path).unwrap();
                file.write_all(&encoded).unwrap();
            }
            ui.horizontal(|ui| {
                ui.label("Min freq:");
                ui.add(egui::Slider::new(&mut self.min_freq, 0.0..=100_000.0).text("Min freq"));
                ui.label("Max freq:");
                ui.add(egui::Slider::new(&mut self.max_freq, 0.0..=100_000.0).text("Max freq"));
            });

            Plot::new("my_plot")
                .legend(Legend::default())
                // .view_aspect(2.0)
                .show(ui, |plot_ui| {
                    for plot_data in &self.plots {
                        let points: Vec<_> = plot_data
                            .freqs
                            .iter()
                            .zip(plot_data.amplitudes.iter())
                            .filter(|(&freq, _)| freq >= self.min_freq && freq <= self.max_freq)
                            .map(|(&freq, &amp)| PlotPoint::new(freq, amp))
                            .collect();
                        plot_ui.line(Line::new(PlotPoints::Owned(points)).name(&plot_data.file_name));
                    }
                    if self.plots.len() > 0 {
                        // create average plot
                        let mut avg_amplitudes = vec![0.0; self.plots[0].amplitudes.len()];
                        for plot_data in &self.plots {
                            for (i, &amp) in plot_data.amplitudes.iter().enumerate() {
                                // add the amp or if it doesn't exist, insert it
                                if let Some(avg_amp) = avg_amplitudes.get_mut(i) {
                                    avg_amp.add_assign(amp);
                                } else {
                                    avg_amplitudes.push(amp);
                                }
                            }
                        }
                        avg_amplitudes.iter_mut().for_each(|amp| *amp /= self.plots.len() as f32);
                        let points: Vec<_> = self
                            .plots[0]
                            .freqs
                            .iter()
                            .zip(avg_amplitudes.iter())
                            .filter(|(&freq, _)| freq >= self.min_freq && freq <= self.max_freq)
                            .map(|(&freq, &amp)| PlotPoint::new(freq, amp))
                            .collect();
                        plot_ui.line(Line::new(PlotPoints::Owned(points)).name("Average"));
                        self.avg_plot = PlotData {
                            freqs: self.plots[0].freqs.clone(),
                            amplitudes: avg_amplitudes,
                            file_name: "average".to_string(),
                        };
                    }
                });
        });
    }

    /// Called by the framework to save state before shutdown.
    /// On Windows its saved here: C:\Users\UserName\AppData\Roaming\Phoenix\data\app.ron
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}
