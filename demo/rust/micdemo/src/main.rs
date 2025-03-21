/*
    Copyright 2021 Picovoice Inc.

    You may not use this file except in compliance with the license. A copy of the license is located in the "LICENSE"
    file accompanying this source.

    Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on
    an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the
    specific language governing permissions and limitations under the License.
*/

use chrono::prelude::*;
use clap::{App, Arg, ArgGroup};
use pv_recorder::RecorderBuilder;
use rhino::RhinoBuilder;
use std::sync::atomic::{AtomicBool, Ordering};

static LISTENING: AtomicBool = AtomicBool::new(false);

#[allow(clippy::too_many_arguments)]
fn rhino_demo(
    audio_device_index: i32,
    access_key: &str,
    context_path: &str,
    sensitivity: Option<f32>,
    endpoint_duration_sec: Option<f32>,
    require_endpoint: Option<bool>,
    model_path: Option<&str>,
    output_path: Option<&str>,
) {
    let mut rhino_builder = RhinoBuilder::new(access_key, context_path);

    if let Some(sensitivity) = sensitivity {
        rhino_builder.sensitivity(sensitivity);
    }

    if let Some(endpoint_duration_sec) = endpoint_duration_sec {
        rhino_builder.endpoint_duration_sec(endpoint_duration_sec);
    }

    if let Some(require_endpoint) = require_endpoint {
        rhino_builder.require_endpoint(require_endpoint);
    }

    if let Some(model_path) = model_path {
        rhino_builder.model_path(model_path);
    }

    let rhino = rhino_builder.init().expect("Failed to create Rhino");

    let recorder = RecorderBuilder::new()
        .device_index(audio_device_index)
        .frame_length(rhino.frame_length() as i32)
        .init()
        .expect("Failed to initialize pvrecorder");
    recorder.start().expect("Failed to start audio recording");

    LISTENING.store(true, Ordering::SeqCst);
    ctrlc::set_handler(|| {
        LISTENING.store(false, Ordering::SeqCst);
    })
    .expect("Unable to setup signal handler");

    println!("Listening for commands...");

    let mut audio_data = Vec::new();
    while LISTENING.load(Ordering::SeqCst) {
        let mut pcm = vec![0; recorder.frame_length()];
        recorder.read(&mut pcm).expect("Failed to read audio frame");

        let is_finalized = rhino.process(&pcm).unwrap();
        if is_finalized {
            let inference = rhino.get_inference().unwrap();
            if inference.is_understood {
                println!("\n[{}] Detected:", Local::now().format("%F %T"));
                println!("{{");
                println!("\tintent : '{}'", inference.intent.unwrap());
                println!("\tslots : {{");
                for (slot, value) in inference.slots.iter() {
                    println!("\t\t{} : {}", slot, value);
                }
                println!("\t}}");
                println!("}}\n");
            } else {
                println!("Did not understand the command");
            }
        }

        if output_path.is_some() {
            audio_data.extend_from_slice(&pcm);
        }
    }

    println!("\nStopping...");
    recorder.stop().expect("Failed to stop audio recording");

    if let Some(output_path) = output_path {
        let wavspec = hound::WavSpec {
            channels: 1,
            sample_rate: rhino.sample_rate(),
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(output_path, wavspec)
            .expect("Failed to open output audio file");
        for sample in audio_data {
            writer.write_sample(sample).unwrap();
        }
    }
}

fn show_audio_devices() {
    let audio_devices = RecorderBuilder::new()
        .init()
        .expect("Failed to initialize pvrecorder")
        .get_audio_devices();
    match audio_devices {
        Ok(audio_devices) => {
            for (idx, device) in audio_devices.iter().enumerate() {
                println!("index: {}, device name: {:?}", idx, device);
            }
        }
        Err(err) => panic!("Failed to get audio devices: {}", err),
    };
}

fn main() {
    let matches = App::new("Picovoice Rhino Rust Mic Demo")
        .group(
            ArgGroup::with_name("actions_group")
            .arg("access_key")
            .arg("context_path")
            .arg("show_audio_devices")
            .required(true)
            .multiple(true)
        )
        .arg(
            Arg::with_name("access_key")
                .long("access_key")
                .value_name("ACCESS_KEY")
                .help("AccessKey obtained from Picovoice Console (https://console.picovoice.ai/)")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("context_path")
            .long("context_path")
            .value_name("PATH")
            .help("Path to Rhino context file (.rhn).")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("model_path")
            .long("model_path")
            .value_name("PATH")
            .help("Path to Rhino model file (.pv).")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("sensitivity")
            .long("sensitivity")
            .value_name("SENSITIVITY")
            .help("Inference sensitivity. The value should be a number within [0, 1]. A higher sensitivity results in fewer misses at the cost of increasing the false alarm rate. If not set 0.5 will be used.")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("endpoint_duration")
            .long("endpoint_duration")
            .value_name("DURATION")
            .help("Endpoint duration in seconds. An endpoint is a chunk of silence at the end of an utterance that marks the end of spoken command. It should be a positive number within [0.5, 5]. If not set 1.0 will be used.")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("require_endpoint")
            .long("require_endpoint")
            .value_name("BOOL")
            .help("If set, Rhino requires an endpoint (chunk of silence) before finishing inference.")
            .takes_value(true)
            .possible_values(["TRUE", "true", "FALSE", "false"])
        )
        .arg(
            Arg::with_name("audio_device_index")
            .long("audio_device_index")
            .value_name("INDEX")
            .help("Index of input audio device.")
            .takes_value(true)
            .default_value("-1")
        )
        .arg(
            Arg::with_name("output_path")
            .long("output_path")
            .value_name("PATH")
            .help("Path to recorded audio (for debugging).")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("show_audio_devices")
            .long("show_audio_devices")
        )
        .get_matches();

    if matches.is_present("show_audio_devices") {
        return show_audio_devices();
    }

    let audio_device_index = matches
        .value_of("audio_device_index")
        .unwrap()
        .parse()
        .unwrap();

    let access_key = matches
        .value_of("access_key")
        .expect("AccessKey is REQUIRED for Rhino operation");

    let context_path = matches.value_of("context_path").unwrap();

    let model_path = matches.value_of("model_path");

    let sensitivity = matches
        .value_of("sensitivity")
        .map(|sensitivity| sensitivity.parse().unwrap());

    let endpoint_duration_sec = matches
        .value_of("endpoint_duration")
        .map(|endpoint_duration| endpoint_duration.parse().unwrap());

    let require_endpoint = matches.value_of("require_endpoint").map(|req| match req {
        "TRUE" | "true" => true,
        "FALSE" | "false" => false,
        _ => unreachable!(),
    });

    let output_path = matches.value_of("output_path");

    rhino_demo(
        audio_device_index,
        access_key,
        context_path,
        sensitivity,
        endpoint_duration_sec,
        require_endpoint,
        model_path,
        output_path,
    );
}
