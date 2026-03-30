<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import '@xterm/xterm/css/xterm.css';

	let {
		paneId,
		apiOrigin = ''
	}: {
		paneId: string;
		/** e.g. `http://localhost:3000` when UI is on :5173 */
		apiOrigin?: string;
	} = $props();

	let termEl: HTMLDivElement;
	let term: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let ws: WebSocket | null = null;
	let connected = $state(false);
	let error = $state('');
	let resizeTimer: ReturnType<typeof setTimeout> | undefined = undefined;

	function postResize(rows: number, cols: number) {
		// FitAddon can report 0×0 before layout settles; still notify PTY so SIGWINCH redraws the prompt
		// (WS subscribers miss pre-connect broadcast output — resize is the usual fix).
		const r = rows > 0 ? rows : 24;
		const c = cols > 0 ? cols : 80;
		const base = resolveHttpOrigin();
		const url = `${base}/api/terminal/pane/${encodeURIComponent(paneId)}/resize`;
		fetch(url, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ rows: r, cols: c })
		}).catch(() => {});
	}

	function scheduleResizeNotify() {
		if (!term) return;
		if (resizeTimer !== undefined) clearTimeout(resizeTimer);
		resizeTimer = setTimeout(() => {
			resizeTimer = undefined;
			postResize(term!.cols, term!.rows);
		}, 80);
	}

	function resolveHttpOrigin(): string {
		if (apiOrigin) return apiOrigin.replace(/\/$/, '');
		if (typeof window === 'undefined') return '';
		const { protocol, hostname, port } = window.location;
		const apiPort = port === '5173' ? '3000' : port;
		return `${protocol}//${hostname}${apiPort ? `:${apiPort}` : ''}`;
	}

	function getWsUrl(id: string): string {
		const base = resolveHttpOrigin();
		const u = new URL(base);
		const wsProto = u.protocol === 'https:' ? 'wss:' : 'ws:';
		const qp = new URLSearchParams({ pane_id: id });
		return `${wsProto}//${u.host}/api/terminal/ws?${qp.toString()}`;
	}

	function connect(id: string) {
		error = '';
		ws?.close();
		const url = getWsUrl(id);
		ws = new WebSocket(url);
		ws.binaryType = 'arraybuffer';

		ws.onopen = () => {
			connected = true;
			fitAddon?.fit();
			scheduleResizeNotify();
			term?.focus();
		};

		ws.onmessage = (ev: MessageEvent) => {
			if (ev.data instanceof ArrayBuffer) {
				term?.write(new Uint8Array(ev.data));
			} else if (typeof ev.data === 'string') {
				term?.write(ev.data);
			}
		};

		ws.onclose = () => {
			connected = false;
			term?.write('\r\n\x1b[90m[disconnected]\x1b[0m\r\n');
		};

		ws.onerror = () => {
			error = 'WebSocket connection failed';
			connected = false;
		};
	}

	onMount(() => {
		term = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			fontFamily: '"JetBrainsMono Nerd Font", "FiraCode Nerd Font", "CaskaydiaCove Nerd Font", "Hack Nerd Font", "MesloLGS NF", ui-monospace, "Cascadia Code", "Fira Code", Menlo, monospace',
			theme: {
				background: '#09090b',
				foreground: '#fafafa',
				cursor: '#fafafa',
				selectionBackground: '#27272a',
				black: '#09090b',
				red: '#ef4444',
				green: '#22c55e',
				yellow: '#eab308',
				blue: '#3b82f6',
				magenta: '#a855f7',
				cyan: '#06b6d4',
				white: '#fafafa'
			}
		});

		fitAddon = new FitAddon();
		term.loadAddon(fitAddon);
		term.open(termEl);
		// Defer initial fit so the container has final layout dimensions
		requestAnimationFrame(() => {
			fitAddon?.fit();
		});

		term.onData((data: string) => {
			if (ws?.readyState === WebSocket.OPEN) {
				ws.send(data);
			}
		});

		const resizeObserver = new ResizeObserver(() => {
			fitAddon?.fit();
			scheduleResizeNotify();
		});
		resizeObserver.observe(termEl);

		connect(paneId);

		return () => {
			if (resizeTimer !== undefined) clearTimeout(resizeTimer);
			resizeObserver.disconnect();
		};
	});

	onDestroy(() => {
		ws?.close();
		term?.dispose();
	});
</script>

<div class="flex h-full w-full min-h-[240px] flex-col rounded-md border border-border bg-[#09090b]">
	{#if error}
		<div class="border-b border-red-500/20 bg-red-500/5 px-3 py-1.5 text-[10px] text-red-500">
			{error}
		</div>
	{/if}
	<div class="min-h-0 min-w-0 flex-1 overflow-hidden p-1" bind:this={termEl}></div>
	<div
		class="flex items-center justify-end gap-2 border-t border-border px-2 py-1 text-[10px] text-muted-foreground"
	>
		<span
			class="inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 font-medium {connected
				? 'bg-green-500/10 text-green-500'
				: 'bg-muted text-muted-foreground'}"
		>
			<span class="h-1 w-1 rounded-full {connected ? 'bg-green-500' : 'bg-muted-foreground'}"></span>
			{connected ? 'Live' : 'Offline'}
		</span>
	</div>
</div>
