<template>
  <div id="getMap">
    <div id="pathData">
      Start point height: {{ marker1Height }} <br />
      Stop point height: {{ marker2Height }}
    </div>
    <!-- add an input field and adds it vue reactive elements-->
    <br />

    <!--<canvas id="c" height = "200" width = "500" v-on:click="test"> </canvas>-->

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

    <br />
    <!-- calls a function to display the map-->

    <template>
      <div id="mapper">
        <canvas id="c" height="200" width="500" v-on:click="placeMarker">
        </canvas>
        <template v-if="displayM1 == true">
          <img
            :src="images.marker1"
            style="width: 1%; height: 5%; position: absolute; z-index=2"
            v-bind:style="{
              left: x1 - 12 + 'px',
              top: y1 - 24 + 'px',
              Zindex: 2,
            }"
          />
        </template>

        <template v-if="displayM2 == true">
          <img
            :src="images.marker1"
            style="width: 1%; height: 5%; position: absolute; z-index=2"
            v-bind:style="{
              left: x2 - 12 + 'px',
              top: y2 - 24 + 'px',
              Zindex: 2,
            }"
          />
        </template>
        <!-- Calls the component DrawCords-->
        <draw-cordinates />
        <!--<map-data />-->
        <!--</div>-->
      </div>
    </template>
    <!--</template>-->
  </div>
</template>

<script>
import DrawCordinates from "./DrawCords.vue";
//import MapData from "./MapData.vue";
import axios from "axios";
import { getRoute } from "route";
import { store, mutations } from "../store.js";
import marker from "images/marker1.png";

export default {
  //defines components used
  components: {
    DrawCordinates,
  },

  data: function () {
    //defines variables in vue reactive element.
    return {
      x1: "10",
      y1: "100",
      x2: "1",
      y2: "1",
      displayM1: false,
      displayM2: false,
      pictureRecived: false,
      map: null,
      map_id: null,
      map_path: "/map/",
      map_link: "",
      selectedMarker: 0,
      mapMenuRender: false,
      mapList: null,
      options: [],
      selected: "PLaceholder",
      mapCanvas: null,
      cWidth: null,
      cHeight: null,
      mapData: null,
      mapTotalHeightDiff: null,
      marker1Height: null,
      marker2Height: null,
      images: {
        marker1: marker,
      },
    };
  },

  methods: {
    accumulatedHeight: function () {
      let height = 0;
      for (let i = 1; i < this.recivedCoordinates.point.length; i++) {
        //find the height of the points
        let point1 = this.getPointHeight(
          store.recivedCoordinates.point[i - 1].x,
          store.recivedCoordinates.point[i - 1].y
        );
        let point2 = this.getPointHeight(
          store.recivedCoordinates.point[i].x,
          store.recivedCoordinates.point[i].y
        );
        //adds the total diffrence to the acumalted height diffrence
        height = height + Math.abs(point1 - point2);
      }
      console.log(height);
    },
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
      this.placeMap(this.mapCanvas);

      //this.mapData =
      this.mapData = this.getMapRes(store.map_id);

      //console.log("mapData", this.mapData);
      //this.mapTotalHeightDiff =this.mapData.max_height - this.mapData.min_height;
      //console.log("hDiff", this.mapTotalHeightDiff);
      //console.log(this.mapData);
    },
    getMapRes: async function (id) {
      let mapDatalink = getRoute("/map/" + id + "/meta");
      let temp = await axios.get(mapDatalink);
      this.mapData = temp.data;
      this.mapTotalHeightDiff =
        this.mapData.max_height - this.mapData.min_height;
      console.log("hDiff", this.mapTotalHeightDiff);

      //console.log("hjhjk",this.mapData);
    },
    placeMap: function (map) {
      var base_image = new Image();
      base_image.src = this.map_link;
      base_image.crossOrigin = "anonymous";

      //set canvas resolution

      base_image.onload = function () {
        let pictureSize = document.getElementById("c");
        var height = base_image.height;
        var width = base_image.width;
        pictureSize.width = width;
        pictureSize.height = height;
        map.drawImage(base_image, 0, 0);
      };
    },
    placeMarker(event) {
      var e = document.getElementById("c");
      var rect = e.getBoundingClientRect();

      mutations.setmapOffSetX(rect.x);
      mutations.setmapOffSetY(rect.y + window.scrollY);

      if (this.selectedMarker == 0) {
        this.x1 = event.clientX;
        this.y1 = event.clientY + window.scrollY;
        this.selectedMarker = 1;

        this.displayM1 = true;

        let cordX1 = Math.round(event.clientX - rect.x);
        let cordY1 = Math.round(event.clientY - rect.y);
        this.marker1Height = Math.round(this.getPointHeight(this.x1, this.y1));

        mutations.setMarker(cordX1, cordY1, 0);
      } else if (this.selectedMarker == 1) {
        this.x2 = event.clientX;
        this.y2 = event.clientY + window.scrollY;

        let cordX2 = Math.round(event.clientX - rect.x);
        let cordY2 = Math.round(event.clientY - rect.y);

        mutations.setMarker(cordX2, cordY2, 1);
        this.selectedMarker = 0;
        this.displayM2 = true;
        //console.log("Stop height",this.getPointHeight(this.x2,this.y2));
        this.marker2Height = Math.round(this.getPointHeight(this.x2, this.y2));
        //var marker2RBG = this.mapCanvas.getImageData(this.x2, this.y2, 1, 1);
        //console.log("RBG2", marker2RBG.data);
      }
    },
    getPointHeight: function (x, y) {
      //get the RBG code from pixel with coords x and y with size 1 by 1 pixel
      let pointRBG = this.mapCanvas.getImageData(x, y, 1, 1);
      //Find the precentage of maximum value
      let precetangeOfHeight = pointRBG.data[0] / 255;
      // multiple the presentage with diffrence between the highest and lowest point
      let pointHeight = precetangeOfHeight * this.mapTotalHeightDiff;
      //We add back the lowest to point to get the accurate height or the height over the sea
      let pointHeightfromSea = pointHeight + this.mapData.min_height;

      return pointHeightfromSea;
    },
  },
  mounted: async function () {
    //request for all available maps
    this.mapList = await axios.get(getRoute("/maps"));

    //Places all recived maps into options that is the options in the dropdown menu
    for (let i = 0; i < this.mapList.data.maps.length; i++) {
      this.options.push({ text: this.mapList.data.maps[i], value: i });
    }
    this.mapMenuRender = true;
    var c = document.getElementById("c");
    var ctx = c.getContext("2d");
    this.mapCanvas = ctx;
    //console.log("this.canvas", this.mapCanvas);
  },
};
</script>
<style>
#getMap {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
.mapper {
  align-items: flex-start;
  position: absolute;
  float: left;
  left: 300px;
}
canvas {
  align-items: flex-start;
  position: absolute;
  /*top: 250px;*/
  left: 300px;

  /*background-color: red;*/
  z-index: 0;
}
.mapcontainer {
  position: relative;
  align-items: flex-start;
  z-index: 0;
}
#pathData {
  align-items: flex-start;
  position: absolute;
  /*float: left;*/
  left: 300px;
  top: 200px;
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
