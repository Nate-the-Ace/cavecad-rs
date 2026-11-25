// An extension point (the phone Settings sections today) is a Registry: a plugin is plain data plus a few
// functions, registered once at start-up, and the code that lists them never names any of them.
// A plugin built for another apiVersion is refused with a reason, never half-loaded.

export interface Plugin {
  readonly id: string;
  readonly apiVersion: number;
}

export class Registry<T extends Plugin> {
  private readonly items = new Map<string, T>();
  private readonly refused: string[] = [];
  private readonly name: string;
  private readonly apiVersion: number;

  constructor(name: string, apiVersion: number) {
    this.name = name;
    this.apiVersion = apiVersion;
  }

  /** Adds a plugin. Returns false (and remembers why) when it was built for another api version. */
  register(p: T): boolean {
    if (p.apiVersion !== this.apiVersion) {
      this.refused.push(`${this.name}: "${p.id}" is for api version ${p.apiVersion}, this app speaks ${this.apiVersion}`);
      return false;
    }
    if (this.items.has(p.id)) throw new Error(`${this.name}: "${p.id}" is already registered`);
    this.items.set(p.id, p);
    return true;
  }

  get(id: string): T | undefined {
    return this.items.get(id);
  }

  /** Every plugin, in the order they were registered. */
  list(): T[] {
    return [...this.items.values()];
  }

  /** Why each refused plugin was refused. */
  refusals(): string[] {
    return [...this.refused];
  }
}
