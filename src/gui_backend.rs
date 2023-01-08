use std::sync::mpsc;
use std::thread;

use log::info;

pub enum Command {
    ListOutputs,
    CaptureScreen,
    CaptureWindow,
    Quit,
}

pub enum CommandResult {
    Outputs(Vec<String>),
    Frame,
}

pub fn run_backend(cmd_rx: mpsc::Receiver<Command>, res_tx: mpsc::Sender<CommandResult>) {
    thread::spawn(move || {
        info!("Start gui backend");
        loop {
            let cmd = cmd_rx.recv().unwrap();
            match cmd {
                Command::ListOutputs => {
                    let outputs = vec![String::from("DP-1"), String::from("eDP-1")];
                    res_tx.send(CommandResult::Outputs(outputs)).unwrap()
                }
                Command::CaptureScreen => {}
                Command::CaptureWindow => {}
                Command::Quit => break,
            }
        }
        info!("Gui backend finished");
    });
}
