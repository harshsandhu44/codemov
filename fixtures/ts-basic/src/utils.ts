import { Config } from "./index";

export function validateConfig(cfg: Config): boolean {
    return cfg.timeout > 0 && cfg.name.length > 0;
}

export function mergeConfigs(base: Config, override: Partial<Config>): Config {
    return { ...base, ...override };
}

export interface ValidationResult {
    valid: boolean;
    errors: string[];
}
