<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';

	export let text: string;
	export let command: string;

	let className: string = '';
	export { className as class };
</script>

<button type="button" class={className} on:click={ async () => await invoke('button_cmd_handler',  { cmd: `${command}`}) }>{text}</button>

<style>
	button {
		background-color: #A28A6F;
		color: white;
		cursor: pointer;
	}

	button:hover {
		background-color: #5E503F;
	}
</style>