<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { Streamdown } from 'svelte-streamdown';
	import { copy } from 'svelte-copy';
	import { streamChat, getConversations, getChatHistory, approveJob, rejectJob } from '$lib/api';
	import { activeSession, refreshPendingApprovalCount } from '$lib/stores';
	import type { Conversation } from '$lib/api';
	import ChatTopBar from '$lib/components/chat/ChatTopBar.svelte';
	import ToolCallCard from '$lib/components/chat/ToolCallCard.svelte';
	import ApprovalCard from '$lib/components/chat/ApprovalCard.svelte';
	import { toast } from 'svelte-sonner';

	interface ToolCallState {
		id: string;
		name: string;
		args: Record<string, unknown>;
		result: string | null;
		isError: boolean;
	}

	interface DisplayMessage {
		role: 'user' | 'assistant' | 'system' | 'tool';
		content: string;
		streaming?: boolean;
		toolCallId?: string;
	}

	let messages: DisplayMessage[] = $state([]);
	let conversations: Conversation[] = $state([]);
	let conversationId: string | undefined = $state(undefined);
	let inputText = $state('');
	let sending = $state(false);
	let error = $state('');
	let messagesContainer: HTMLDivElement | undefined = $state(undefined);
	let textareaEl: HTMLTextAreaElement | undefined = $state(undefined);
	let toolCalls: Map<string, ToolCallState> = $state(new Map());
	let currentSessionId = $state<string | null>(null);
	activeSession.subscribe((s) => (currentSessionId = s?.id ?? null));

	onMount(async () => {
		try {
			conversations = await getConversations();
		} catch {
			// No conversations yet
		}
		textareaEl?.focus();
	});

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	function autoResize() {
		if (textareaEl) {
			textareaEl.style.height = 'auto';
			textareaEl.style.height = Math.min(textareaEl.scrollHeight, 200) + 'px';
		}
	}

	async function loadConversation(id: string) {
		try {
			const history = await getChatHistory(id);
			messages = history.map((m) => ({ role: m.role, content: m.content }));
			conversationId = id;
			await scrollToBottom();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load conversation';
			toast.error(error);
		}
	}

	function newConversation() {
		messages = [];
		conversationId = undefined;
		inputText = '';
		error = '';
		toolCalls = new Map();
	}

	async function sendMessage() {
		const text = inputText.trim();
		if (!text || sending) return;

		inputText = '';
		if (textareaEl) textareaEl.style.height = 'auto';
		sending = true;
		error = '';

		messages = [...messages, { role: 'user', content: text }];
		await scrollToBottom();

		messages = [...messages, { role: 'assistant', content: '', streaming: true }];
		await scrollToBottom();

		try {
			await streamChat(
				text,
				conversationId,
				currentSessionId,
				(deltaText, convId) => {
					conversationId = convId;
					const last = messages[messages.length - 1];
					if (last?.role === 'assistant') {
						messages = [...messages.slice(0, -1), { ...last, content: last.content + deltaText }];
					}
					scrollToBottom();
				},
				(fullText, convId) => {
					conversationId = convId;
					messages = [
						...messages.slice(0, -1),
						{ role: 'assistant', content: fullText, streaming: false }
					];
					sending = false;
					getConversations()
						.then((c) => (conversations = c))
						.catch(() => {});
				},
				(msg) => {
					error = msg;
					if (messages[messages.length - 1]?.content === '') {
						messages = messages.slice(0, -1);
					}
					sending = false;
				},
				(id, name, args, convId) => {
					conversationId = convId;
					toolCalls = new Map(toolCalls.set(id, { id, name, args, result: null, isError: false }));
					messages = [...messages, { role: 'tool', content: '', toolCallId: id }];
					scrollToBottom();
				},
				(id, name, result, isError, convId) => {
					conversationId = convId;
					const existing = toolCalls.get(id);
					if (existing) {
						toolCalls = new Map(toolCalls.set(id, { ...existing, result, isError }));
					} else {
						toolCalls = new Map(toolCalls.set(id, { id, name, args: {}, result, isError }));
					}
					scrollToBottom();
				}
			);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to send message';
			toast.error(error);
			if (messages[messages.length - 1]?.content === '') {
				messages = messages.slice(0, -1);
			}
			sending = false;
		}
	}

	function handleApprove(jobId: string) {
		approveJob(jobId)
			.then(() => {
				toast.success('Approved. The job will continue.');
				return refreshPendingApprovalCount();
			})
			.catch((e) => {
				toast.error(e instanceof Error ? e.message : 'Approve failed');
			});
	}
	function handleReject(jobId: string) {
		rejectJob(jobId)
			.then(() => {
				toast.success('Rejected. The job was cancelled.');
				return refreshPendingApprovalCount();
			})
			.catch((e) => {
				toast.error(e instanceof Error ? e.message : 'Reject failed');
			});
	}
	function isApprovalResult(tc: ToolCallState): boolean {
		return tc.result !== null && !tc.isError && tc.result.includes('awaiting_approval');
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			sendMessage();
		}
	}

	function formatTime(conv: Conversation): string {
		try {
			const d = new Date(conv.updated_at);
			const now = new Date();
			if (d.toDateString() === now.toDateString()) {
				return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
			}
			return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
		} catch {
			return '';
		}
	}
</script>

<div class="flex h-full">
	<!-- Conversation sidebar -->
	<div class="flex w-60 flex-shrink-0 flex-col border-r border-border bg-sidebar">
		<div class="border-b border-border p-3">
			<button
				onclick={newConversation}
				class="flex w-full items-center justify-center gap-2 rounded-lg bg-primary px-3 py-2.5 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
			>
				<span class="text-lg leading-none">+</span> New Chat
			</button>
		</div>
		<div class="flex-1 overflow-y-auto p-2">
			{#if conversations.length === 0}
				<p class="p-3 text-center text-xs text-muted-foreground/50">
					No conversations yet.<br />Start chatting below.
				</p>
			{:else}
				{#each conversations as conv}
					<button
						onclick={() => loadConversation(conv.id)}
						class="mb-1 w-full rounded-lg px-3 py-2.5 text-left transition-colors
							{conversationId === conv.id
							? 'bg-sidebar-primary/15 border border-sidebar-primary/30 text-foreground'
							: 'border border-transparent text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground'}"
					>
						<p class="truncate text-sm">{conv.title}</p>
						<div class="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground/50">
							<span>{conv.message_count} msgs</span>
							<span>{formatTime(conv)}</span>
						</div>
					</button>
				{/each}
			{/if}
		</div>
	</div>

	<!-- Chat area -->
	<div class="flex flex-1 flex-col">
		<ChatTopBar />

		<!-- Messages -->
		<div bind:this={messagesContainer} class="flex-1 overflow-y-auto">
			{#if messages.length === 0}
				<div class="flex h-full items-center justify-center p-6">
					<div class="text-center">
						<div
							class="mx-auto mb-4 flex h-20 w-20 items-center justify-center rounded-2xl bg-gradient-to-br from-primary/30 to-chart-4/20"
						>
							<span class="text-3xl font-bold text-primary">R</span>
						</div>
						<h2 class="text-xl font-semibold text-foreground">RUSVEL Assistant</h2>
						<p class="mt-2 max-w-sm text-sm leading-relaxed text-muted-foreground">
							Your AI companion that knows your products, skills, and mission. Plan your day, draft
							content, strategize, or just think out loud.
						</p>
						<div class="mt-6 flex flex-wrap justify-center gap-2">
							{#each ['Plan my day', 'Draft a blog post', 'Review my goals', 'What should I focus on?'] as suggestion}
								<button
									onclick={() => {
										inputText = suggestion;
										sendMessage();
									}}
									class="rounded-full border border-border bg-secondary/50 px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:border-primary/50 hover:text-foreground"
								>
									{suggestion}
								</button>
							{/each}
						</div>
					</div>
				</div>
			{:else}
				<div class="mx-auto max-w-3xl space-y-1 p-6">
					{#each messages as msg, i}
						{#if msg.role === 'tool' && msg.toolCallId}
							{@const tc = toolCalls.get(msg.toolCallId)}
							{#if tc}
								<div class="flex gap-3 justify-start mt-1">
									<div class="w-8 flex-shrink-0"></div>
									<div class="max-w-[85%]">
										<ToolCallCard
											name={tc.name}
											args={tc.args}
											result={tc.result}
											isError={tc.isError}
										/>
										{#if isApprovalResult(tc)}
											{@const approvalData = (() => { try { return JSON.parse(tc.result ?? '{}'); } catch { return {}; } })()}
											<ApprovalCard
												jobId={approvalData.job_id ?? tc.id}
												jobKind={tc.name}
												payload={tc.args}
												onApprove={handleApprove}
												onReject={handleReject}
												terminalDeptId={
													typeof approvalData.terminal_dept_id === 'string'
														? approvalData.terminal_dept_id
														: undefined
												}
											/>
										{/if}
									</div>
								</div>
							{/if}
						{:else}
						{@const isUser = msg.role === 'user'}
						{@const showAvatar = i === 0 || messages[i - 1]?.role !== msg.role}
						<div
							class="flex gap-3 {isUser ? 'justify-end' : 'justify-start'} {showAvatar
								? 'mt-4'
								: 'mt-1'}"
						>
							{#if !isUser && showAvatar}
								<div
									class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-primary/40 to-chart-4/30 text-xs font-bold text-primary"
								>
									R
								</div>
							{:else if !isUser}
								<div class="w-8 flex-shrink-0"></div>
							{/if}

							{#if isUser}
								<div
									class="max-w-[75%] rounded-2xl rounded-br-md bg-primary px-4 py-2.5 text-sm text-primary-foreground"
								>
									<p class="whitespace-pre-wrap">{msg.content}</p>
								</div>
								{#if showAvatar}
									<div
										class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-secondary text-xs font-bold text-muted-foreground"
									>
										M
									</div>
								{:else}
									<div class="w-8 flex-shrink-0"></div>
								{/if}
							{:else}
								<div
									class="max-w-[85%] rounded-2xl rounded-bl-md bg-secondary px-4 py-3 text-sm text-foreground relative group"
								>
									{#if msg.streaming && !msg.content}
										<div class="flex items-center gap-1.5">
											<div
												class="h-2 w-2 animate-bounce rounded-full bg-primary"
												style="animation-delay: 0ms"
											></div>
											<div
												class="h-2 w-2 animate-bounce rounded-full bg-primary"
												style="animation-delay: 150ms"
											></div>
											<div
												class="h-2 w-2 animate-bounce rounded-full bg-primary"
												style="animation-delay: 300ms"
											></div>
										</div>
									{:else}
										<Streamdown
											content={msg.content}
											parseIncompleteMarkdown={!!msg.streaming}
											baseTheme="shadcn"
											animation={{
												enabled: !!msg.streaming,
												type: 'blur',
												duration: 300,
												tokenize: 'word'
											}}
										/>
										{#if msg.streaming}
											<span class="inline-block h-4 w-0.5 animate-pulse bg-primary"></span>
										{/if}
									{/if}
									{#if msg.content && !msg.streaming}
										<button
											use:copy={msg.content}
											onclick={() => toast.success('Copied')}
											class="absolute top-1 right-1 hidden group-hover:flex h-5 w-5 items-center justify-center rounded bg-secondary text-muted-foreground hover:text-foreground text-[10px]"
											title="Copy message"
										>
											<svg
												class="h-3 w-3"
												viewBox="0 0 16 16"
												fill="none"
												stroke="currentColor"
												stroke-width="1.5"
												><rect x="5" y="5" width="8" height="8" rx="1" /><path d="M3 11V3h8" /></svg
											>
										</button>
									{/if}
								</div>
							{/if}
						</div>
						{/if}
					{/each}
				</div>
			{/if}
		</div>

		<!-- Error -->
		{#if error}
			<div
				class="mx-6 mb-2 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive"
			>
				{error}
				<button onclick={() => (error = '')} class="ml-2 text-destructive hover:text-destructive/80"
					>dismiss</button
				>
			</div>
		{/if}

		<!-- Input -->
		<div class="border-t border-border bg-background/50 p-4">
			<div class="mx-auto flex max-w-3xl items-end gap-3">
				<textarea
					bind:this={textareaEl}
					bind:value={inputText}
					onkeydown={handleKeydown}
					oninput={autoResize}
					placeholder="Message RUSVEL..."
					rows="1"
					disabled={sending}
					class="flex-1 resize-none rounded-xl border border-input bg-secondary px-4 py-3 text-sm leading-relaxed text-foreground placeholder-muted-foreground focus:border-primary focus:outline-none focus:ring-1 focus:ring-ring/30 disabled:opacity-50"
				></textarea>
				<button
					onclick={sendMessage}
					disabled={sending || !inputText.trim()}
					class="flex h-11 w-11 flex-shrink-0 items-center justify-center rounded-xl bg-primary text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-30"
				>
					{#if sending}
						<div
							class="h-4 w-4 animate-spin rounded-full border-2 border-primary-foreground/30 border-t-primary-foreground"
						></div>
					{:else}
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="h-4 w-4"
							viewBox="0 0 20 20"
							fill="currentColor"
						>
							<path
								d="M10.894 2.553a1 1 0 00-1.788 0l-7 14a1 1 0 001.169 1.409l5-1.429A1 1 0 009 15.571V11a1 1 0 112 0v4.571a1 1 0 00.725.962l5 1.428a1 1 0 001.17-1.408l-7-14z"
							/>
						</svg>
					{/if}
				</button>
			</div>
		</div>
	</div>
</div>
