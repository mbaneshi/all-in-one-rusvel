/**
 * Terminal HTTP/WebSocket URLs from the browser.
 * Always same-origin `/api/...` so Vite dev (e.g. :5173) uses the proxy to 127.0.0.1:3000.
 * Avoids `fetch('http://localhost:3000/...')` which can hang when `localhost` resolves to ::1
 * but the API is bound to IPv4 only.
 */
export function terminalDeptPaneUrl(dept: string, sessionId: string): string {
	return `/api/terminal/dept/${encodeURIComponent(dept)}?session_id=${encodeURIComponent(sessionId)}`;
}

export function terminalPaneResizeUrl(paneId: string): string {
	return `/api/terminal/pane/${encodeURIComponent(paneId)}/resize`;
}

export function terminalWebSocketUrl(paneId: string): string {
	if (typeof window === 'undefined') return '';
	const p = new URLSearchParams({ pane_id: paneId });
	const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${proto}//${window.location.host}/api/terminal/ws?${p.toString()}`;
}
