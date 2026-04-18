import { readFileSync } from "fs";

export interface AppConfig {
    host: string;
    port: number;
}

export function loadConfig(path: string): AppConfig {
    const raw = readFileSync(path, "utf-8");
    return JSON.parse(raw) as AppConfig;
}

export const DEFAULT_PORT = 8080;
