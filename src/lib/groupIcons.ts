/** Fixed icon set for app groups. Paths are stroked in a 24×24 view box. */
export const GROUP_ICONS = ["apps", "browser", "video", "game", "music", "chat", "folder", "star"] as const;

export type GroupIcon = (typeof GROUP_ICONS)[number];

export const GROUP_COLORS = ["#3d8bfd", "#2fbf71", "#e0a100", "#e85d4c", "#9b6dff", "#e07a3d", "#3db8c9", "#6b7280"];

const PATHS: Record<GroupIcon, string> = {
    apps: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    browser: "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM4 12h16M12 4c2.5 2.4 3.8 5.2 3.8 8S14.5 17.6 12 20c-2.5-2.4-3.8-5.2-3.8-8S9.5 6.4 12 4z",
    video: "M5 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM16 10l4-2v8l-4-2z",
    game: "M7 9h10a4 4 0 0 1 4 4 4 4 0 0 1-4 4h-1l-1.5-2h-4L9 17H7a4 4 0 0 1-4-4 4 4 0 0 1 4-4zM8 12v2M7 13h2M16 13h.01M18 12h.01",
    music: "M9 18a3 3 0 1 1 0-6 3 3 0 0 1 0 6zM12 15V6l8-2v9M20 10a3 3 0 1 1 0-6",
    chat: "M5 6h14a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H9l-4 3v-3H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1z",
    folder: "M3 7a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
    star: "M12 3.5l2.4 5 5.6.8-4 3.9.9 5.5L12 16.2 7.1 18.7 8 13.2 4 9.3l5.6-.8z",
};

export function iconPath(icon: string): string {
    return icon in PATHS ? PATHS[icon as GroupIcon] : PATHS.apps;
}
