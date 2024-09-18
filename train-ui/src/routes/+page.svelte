<script lang="ts">
  import ConfigPanel from "$lib/ConfigPanel.svelte";
  import Visualisation from "$lib/Visualisation.svelte";

  let visualisation: Visualisation;
</script>

<svelte:head>
  <title>Train Ute Model</title>
</svelte:head>

<div id="container">
  <h1>Train Ute Model</h1>
  <div id="panels">
    <div id="left-panel">
      <Visualisation bind:this={visualisation}/>
    </div>
    <div id="right-panel">
      <ConfigPanel on:simulation-finished={() => visualisation.updateData()}/>
    </div>
  </div>
</div>

<style>
  :root {
    background-color: #171c23;
  }

  #container {
    height: 100vh;
    width: 100%;
    display: flex;
    flex-direction: column;
    padding: 15px;
  }

  h1 {
    text-align: center;
    color: #eae0d5;
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
