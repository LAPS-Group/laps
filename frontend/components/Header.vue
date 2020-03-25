<template>
  <div id="header">
    <!--<img :src="images.Laps">-->
  </div>
</template>
<script>
//import laps_logo from "./LAPS1.png";
/*
function component() {
  const element = document.createElement("div");
  const myIcon = new Image();
  myIcon.src = laps_logo;

  element.appendChild(myIcon);
 
  return element;
}
document.body.appendChild(component());
 document.getElementById("myIcon").width = "300"; 
 */
export default {
  data() {
    return {
      images: {
        Laps: require("./LAPS1.png")
      }
    };
  }
};
</script>
