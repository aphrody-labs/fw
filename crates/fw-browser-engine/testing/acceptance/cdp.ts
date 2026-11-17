// Raw Chrome DevTools Protocol over the runtime WebSocket: one connection, flat sessions, no dependency.
export type CdpMessage = {
  id?: number;
  method?: string;
  params?: any;
  sessionId?: string;
  result?: any;
  error?: { message: string };
};

export class Cdp {
  #ws: WebSocket;
  #id = 0;
  #pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void; timer: Timer }>();
  #listeners: ((message: CdpMessage) => void)[] = [];

  static open(url: string): Promise<Cdp> {
    return new Promise((done, fail) => {
      const ws = new WebSocket(url);
      ws.onopen = () => done(new Cdp(ws));
      ws.onerror = () => fail(new Error(`no CDP connection to ${url}`));
    });
  }

  private constructor(ws: WebSocket) {
    this.#ws = ws;
    ws.onmessage = (event) => {
      const message = JSON.parse(String(event.data)) as CdpMessage;
      const call = message.id === undefined ? undefined : this.#pending.get(message.id);
      if (!call) return void this.#listeners.forEach((fn) => fn(message));
      this.#pending.delete(message.id!);
      clearTimeout(call.timer);
      if (message.error) call.reject(new Error(message.error.message));
      else call.resolve(message.result);
    };
    ws.onclose = () => {
      for (const call of this.#pending.values()) {
        clearTimeout(call.timer);
        call.reject(new Error("CDP connection closed"));
      }
      this.#pending.clear();
    };
  }

  send<T = any>(method: string, params: object = {}, sessionId?: string, timeout = 30000): Promise<T> {
    return new Promise((resolve, reject) => {
      const id = ++this.#id;
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`${method} got no answer in ${timeout} ms`));
      }, timeout);
      this.#pending.set(id, { resolve, reject, timer });
      this.#ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  }

  on(fn: (message: CdpMessage) => void) {
    this.#listeners.push(fn);
  }

  close() {
    this.#ws.close();
  }
}
