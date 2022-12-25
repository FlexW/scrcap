use anyhow::{bail, Result};
use log::debug;
use swayipc::Connection;

pub fn active_window_area() -> Result<(i32, i32, i32, i32)> {
    let mut connection = Connection::new()?;
    let tree = connection.get_tree()?;
    let focused_node = tree.find_focused_as_ref(|node: _| node.focused);
    if let Some(focused_node) = focused_node {
        let rect = &focused_node.rect;
        let window_rect = &focused_node.window_rect;

        let x = rect.x + window_rect.x;
        let y = rect.y + window_rect.y;
        let width = window_rect.width;
        let height = window_rect.height;

        debug!(
            "Focused window: {:?} x:{}, y: {}, width: {}, height: {}",
            focused_node.name, x, y, width, height
        );

        return Ok((x, y, width, height));
    }

    bail!("Could not find an active window")
}
