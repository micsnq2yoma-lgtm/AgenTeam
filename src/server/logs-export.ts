/** File name of the exported run-log archive. Product name first, so a user who sends it for support
 *  sees the app they are actually running (it used to say MyAgents, the upstream project's name). */
export function logsExportZipName(now: Date = new Date()): string {
  return `AgenTeam-logs-${now.toISOString().slice(0, 10)}.zip`;
}
