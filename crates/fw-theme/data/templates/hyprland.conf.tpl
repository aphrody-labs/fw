# fw-theme {{ name }} ({{ mode }}): Hyprland colors. In hyprland.conf: source = ~/.local/state/fw/theme/current/hyprland-colors.conf
$fw_background = {{ background | hypr }}
$fw_foreground = {{ foreground | hypr }}
$fw_accent = {{ accent | hypr }}
$fw_muted = {{ muted | hypr }}
$fw_selection = {{ selection | hypr }}

general {
    col.active_border = $fw_accent
    col.inactive_border = $fw_muted
}

group {
    col.border_active = $fw_accent
    col.border_inactive = $fw_muted
    groupbar {
        col.active = $fw_accent
        col.inactive = $fw_muted
    }
}

decoration {
    shadow {
        color = {{ darker_background | hyprwa }}
    }
}

misc {
    background_color = $fw_background
}
