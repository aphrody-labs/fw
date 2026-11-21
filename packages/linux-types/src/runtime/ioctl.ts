// ioctl request descriptors: the number decoded into its _IOC fields and the argument it names.
import type { AnyLayout, Layout } from "./layout.ts";

// _IOC_NONE, _IOC_WRITE (userspace writes, kernel reads), _IOC_READ as in asm-generic/ioctl.h.
export const IOC_NONE = 0;
export const IOC_WRITE = 1;
export const IOC_READ = 2;

export interface Ioctl<A = unknown> {
  readonly request: number;
  // 0 none, 1 write, 2 read, 3 read and write. Legacy numbers (TIOCGWINSZ, ...) carry 0 and size 0.
  readonly dir: number;
  readonly type: number;
  readonly nr: number;
  // The size field of the request number.
  readonly size: number;
  // The argument type as the macro spells it (`struct winsize`), or "" when the number is legacy.
  readonly argType: string;
  // The layout of the argument when it is a struct or union.
  readonly arg: Layout<A> | null;
}

export function ioctlDef<A = never>(request: number, dir: number, type: number, nr: number, size: number, argType = "", arg: (() => Layout<A>) | null = null): Ioctl<A> {
  return {
    request,
    dir,
    type,
    nr,
    size,
    argType,
    get arg() {
      return arg ? arg() : null;
    },
  };
}

export type IoctlArg<I> = I extends Ioctl<infer A> ? A : never;
export type { AnyLayout };
