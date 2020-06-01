<template>
  <div id="loginLink">
    <button v-on:click="redirect">Admin Panel</button>
  </div>
</template>
<script>
export default {
  methods: {
    redirect: function () {
      window.location.href = "https://laps.website/login";
    },
  },
};
</script>
