// The parts of @polyph/ui with no React: the daemon client, strings and
// approval sentences. The VS Code extension host imports this entry.
export {
  DaemonClient,
  RpcError,
  type AppClient,
  type ConnState,
  type Transport,
} from "./api/client";
export type * from "./api/types";
export { Localizer, pickLang, LANGS } from "./i18n/localizer";
export { describeApproval } from "./describe";
