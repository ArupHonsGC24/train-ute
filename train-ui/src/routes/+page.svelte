<script>
	import ConfigPanel from '$lib/components/ConfigPanel.svelte';
	import Visualisation from '$lib/components/Visualisation.svelte';
</script>

<h1 id="title">Train Ute Model</h1>
<div id="panels">
	<div id="left-panel">
		<Visualisation />
	</div>
	<div id="right-panel">
		<ConfigPanel />
	</div>
</div>

<style>
	:root {
		background-color: #171C23;
	}

	#title {
		text-align: center;
		color: #EAE0D5;
		margin: 10px 0;
		font-size: 2rem;
	}

	#panels {
		display: flex;
		height: 100%;
	}

	#left-panel {
		flex: 3;
		padding: 20px;
	}

	#right-panel {
		flex: 2;
		padding: 20px 40px 20px 20px;
	}
</style>
