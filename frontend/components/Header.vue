<template>
  <div id="header">
    <img :src="images.Laps" style="width:20%;height:20%;" />
    <br />
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
<style>
#img {
  width: 30%;
  height: 30%;
}
</style>
