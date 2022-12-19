use wayland_client::{Display, GlobalManager};

fn main() {
    // Connect to the server
    let display = Display::connect_to_env().unwrap();

    let mut event_queue = display.create_event_queue();

    let attached_display = (*display).clone().attach(event_queue.token());

    // We use the GlobalManager convenience provided by the crate, it covers
    // most classic use cases and avoids us the trouble to manually implement
    // the registry
    let globals = GlobalManager::new(&attached_display);

    // A roundtrip synchronization to make sure the server received our registry
    // creation and sent us the global list
    event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!()).unwrap();

    // Print the list
    for (id, interface, version) in globals.list() {
        println!("{}: {} (version {})", id, interface, version);
    }
}
