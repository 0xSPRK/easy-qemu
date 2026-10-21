declare module "@novnc/novnc" {
  export interface RfbOptions {
    shared?: boolean;
    credentials?: { password?: string };
  }

  export default class RFB extends EventTarget {
    constructor(target: HTMLElement, urlOrChannel: string | WebSocket, options?: RfbOptions);
    disconnect(): void;
    sendKey(keysym: string, code: string, down: boolean): void;
    sendCtrlAltDel(): void;
    focus(options?: { preventScroll?: boolean }): void;
    blur(): void;
    get scaleViewport(): boolean;
    set scaleViewport(v: boolean);
    get resizeSession(): boolean;
    set resizeSession(v: boolean);
    get viewOnly(): boolean;
    set viewOnly(v: boolean);
    get background(): string;
    set background(v: string);
  }
}
