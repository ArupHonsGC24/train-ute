<script lang="ts">
	import { onMount } from 'svelte';

	let canvas: HTMLCanvasElement;

	onMount(() => {
		canvas.getContext('2d', { alpha: false });
	});
</script>

<canvas bind:this={canvas}/>

<style>
	canvas {
		width: 100%;
		height: 100%;
		background-color: #22333B;
		border-radius: 15px;
		border: 1px solid #5E503F;
	}
</style>