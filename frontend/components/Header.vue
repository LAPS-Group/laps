<template>
  <div id="header">
    <img :src="images.Laps" />
  </div>
</template>
<script>
import laps_logo from "images/LAPS1.png";

export default {
  data() {
    return {
      images: {
        Laps: laps_logo
      }
    };
  }
};
</script>
