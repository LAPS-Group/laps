<template>
  <div id="getMap">
    <!-- add an input field and adds it vue reactive elements-->
    <br />
    Select Map
    <br />
    <!-- creates a dropdown menu for maps -->
    <template v-if="mapMenuRender == true">
      <select v-model="selected" @change="onChange($event)" class="drop-down">
        <option v-for="option in options" v-bind:value="option.value">
          {{ option.text }}
        </option>
      </select>
    </template>
    <!--<input v-model="map_id" @change="getMap" />-->
    <br />
    <!-- calls a function to display the map-->
    <!--<button v-on:click="getMap">Get Map</button>-->

    <!-- creates a new template wich is only displayed if a map is recived-->

    <template v-if="pictureRecived == true"
      ><br />

      <div class="map">
        <div class="mapcontainer"><img :src="this.map_link" />--></div>
        <!-- Calls the component DrawCords-->
        <draw-cordinates />
      </div>
    </template>
  </div>
</template>
<script>
import DrawCordinates from "./DrawCords.vue";
import axios from "axios";
import { getRoute } from "route";
import { store, mutations } from "../store.js";

export default {
  //defines components used
  components: {
    DrawCordinates,
  },

  data: function () {
    //defines variables in vue reactive element.
    return {
      pictureRecived: false,
      map: null,
      map_id: null,
      map_path: "/map/",
      map_link: "",

      mapMenuRender: false,
      mapList: null,
      options: [],
      selected: "PLaceholder",
    };
  },

  methods: {
    // displays the map based on ID
    getMap: function () {
      this.map_link = getRoute(this.map_path + this.map_id);

      // Is set to true so that, it doesn't try display maps without a map
      this.pictureRecived = true;
      // send map ID to the store
      mutations.setmap_id(this.map_id);
    },
    //run after a new option is selected in the dropdown menu
    onChange(event) {
      //takes the selected option and generate the request path
      this.map_link = getRoute(
        this.map_path + this.mapList.data.maps[this.selected]
      );
      //send the map id to the store to be used by other places that needs it
      mutations.setmap_id(this.mapList.data.maps[this.selected]);
      this.pictureRecived = true;
    },
  },
  mounted: async function () {
    //request for all available maps
    this.mapList = await axios.get(getRoute("/maps"));
    console.log(JSON.stringify(this.mapList.data.maps[0]));
    //Places all recived maps into options that is the options in the dropdown menu
    for (let i = 0; i < this.mapList.data.maps.length; i++) {
      this.options.push({ text: this.mapList.data.maps[i], value: i });
      console.log(i);
    }
    this.mapMenuRender = true;
  },
};
</script>
<style>
#getMap {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
canvas {
  align-items: flex-start;
  position: absolute;
  top: 0px;
  left: 0px;

  background-color: red;
  z-index: 1;
}
.mapcontainer {
  position: absolute;
  align-items: flex-start;
}
.map {
  align-items: flex-start;
  position: relative;
  float: left;
  left: 300px;
}
.drop-down {
  display: block;
  /*font-size: 16px;
	font-family: sans-serif;
	font-weight: 700;
	color: #444;
	line-height: 1.3;
	padding: .6em 1.4em .5em .8em;
  */
  width: 175px;
  /*
	box-sizing: border-box;
	margin: 0;
	border: 1px solid #aaa;
	box-shadow: 0 1px 0 1px rgba(0,0,0,.04);
	border-radius: .5em;
	-moz-appearance: none;
	-webkit-appearance: none;
	appearance: none;
	background-color: #fff;
  */
}
</style>
