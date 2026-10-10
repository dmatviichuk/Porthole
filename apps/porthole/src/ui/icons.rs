pub fn svg(name: &str) -> &'static [u8] {
    match name {
        "arrow-down-to-line" => include_bytes!("../../../../assets/icons/arrow-down-to-line.svg"),
        "chart-no-axes-column" => {
            include_bytes!("../../../../assets/icons/chart-no-axes-column.svg")
        }
        "check" => include_bytes!("../../../../assets/icons/check.svg"),
        "chevron-down" => include_bytes!("../../../../assets/icons/chevron-down.svg"),
        "chevron-left" => include_bytes!("../../../../assets/icons/chevron-left.svg"),
        "chevron-right" => include_bytes!("../../../../assets/icons/chevron-right.svg"),
        "chevron-up" => include_bytes!("../../../../assets/icons/chevron-up.svg"),
        "chevrons-up-down" => include_bytes!("../../../../assets/icons/chevrons-up-down.svg"),
        "circle-dashed" => include_bytes!("../../../../assets/icons/circle-dashed.svg"),
        "clock" => include_bytes!("../../../../assets/icons/clock.svg"),
        "clock-3" => include_bytes!("../../../../assets/icons/clock-3.svg"),
        "folder" => include_bytes!("../../../../assets/icons/folder.svg"),
        "keyboard" => include_bytes!("../../../../assets/icons/keyboard.svg"),
        "layers" => include_bytes!("../../../../assets/icons/layers.svg"),
        "layout-grid" => include_bytes!("../../../../assets/icons/layout-grid.svg"),
        "minus" => include_bytes!("../../../../assets/icons/minus.svg"),
        "monitor" => include_bytes!("../../../../assets/icons/monitor.svg"),
        "moon" => include_bytes!("../../../../assets/icons/moon.svg"),
        "more-horizontal" => include_bytes!("../../../../assets/icons/more-horizontal.svg"),
        "pause" => include_bytes!("../../../../assets/icons/pause.svg"),
        "rotate-cw" => include_bytes!("../../../../assets/icons/rotate-cw.svg"),
        "search" => include_bytes!("../../../../assets/icons/search.svg"),
        "square-terminal" => include_bytes!("../../../../assets/icons/square-terminal.svg"),
        "sun" => include_bytes!("../../../../assets/icons/sun.svg"),
        "trash-2" => include_bytes!("../../../../assets/icons/trash-2.svg"),
        "triangle-alert" => include_bytes!("../../../../assets/icons/triangle-alert.svg"),
        "x" => include_bytes!("../../../../assets/icons/x.svg"),
        _ => include_bytes!("../../../../assets/icons/minus.svg"),
    }
}
