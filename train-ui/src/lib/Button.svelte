<script lang="ts">
  export let text: string;
  export let defaultTooltip = "";
  export let disabledTooltip = "";
  export let disabled = false;
  export let processIndicator = false;
  export let processComplete = false;

  let className = "";
  // noinspection JSUnusedGlobalSymbols, ReservedWordAsName
  export { className as class };

  $: title = disabled ? disabledTooltip : defaultTooltip;
  $: indicatorTitle = processComplete ? "Complete" : "Incomplete";
</script>

<button type="button" {title} class={className} {disabled} on:click><span class="label">{text}</span>
  {#if processIndicator}
    {#if processComplete}
      <span title={indicatorTitle} class="indicator indicatorComplete">✔️</span>
    {:else}
      <span title={indicatorTitle} class="indicator indicatorIncomplete" />
    {/if}
  {/if}
</button>

<style>
  button {
    background-color: #a28a6f;
    color: white;
    display: flex;
  }

  button:disabled {
    background-color: #5e503f;
    color: #a28a6f;
  }

  button:hover:enabled {
    background-color: #5e503f;
  }

  .label {
    flex: 9
  }

  .indicator {
    flex: 0 0 auto;
    align-self: center;
    height: 15px;
    width: 15px;
    margin: 5px;
    border-radius: 50%;
  }

  .indicatorIncomplete {
    background-color: #bbb;
  }

  .indicatorComplete {
    background-color: transparent;
    font-size: 10px;
    color: transparent;
    text-shadow: 0 0 0 green;
  }
</style>
