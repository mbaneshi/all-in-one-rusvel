// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

interface ImportMetaEnv {
	/** Optional bearer for API when `RUSVEL_API_TOKEN` is set on the server (dev / static build). */
	readonly VITE_RUSVEL_API_TOKEN?: string;
}

export {};
