import { EventEmitter } from "events";
import path from "path";

export interface Config {
    debug: boolean;
    timeout: number;
    name: string;
}

export type Handler = (event: string, data: unknown) => void;

export class EventBus {
    private emitter: EventEmitter;

    constructor() {
        this.emitter = new EventEmitter();
    }

    on(event: string, handler: Handler): void {
        this.emitter.on(event, handler);
    }

    emit(event: string, data: unknown): void {
        this.emitter.emit(event, data);
    }
}

export function createConfig(name: string): Config {
    return { debug: false, timeout: 5000, name };
}

export const DEFAULT_TIMEOUT = 5000;

export const formatPath = (p: string): string => path.normalize(p);
