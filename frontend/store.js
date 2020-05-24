import Vue from "vue";
//store variables
export const store = Vue.observable({
  tester: 0,
  recivedCoordinates: { test: "noe" },
  selected_algorithms: [],
  job_token: null,
  map_id: null,
  pictureRecived: false,
  markers: [
    { x: null, y: null },
    { x: null, y: null },
  ],
  mapOffSetX:null,
  mapOffSetY:null,
  startHeight:null,
  stopHeight:null,
});
//function to update a variable
export const mutations = {
  settester(tester) {
    store.tester = tester;
  },
  setrecivedCoordinates(recivedCoordinates) {
    store.recivedCoordinates = recivedCoordinates;
  },
  setselected_algorithms(selected_algorithms) {
    store.selected_algorithms = selected_algorithms;
  },
  setjob_token(job_token) {
    store.job_token = job_token;
  },
  setmap_id(map_id) {
    store.map_id = map_id;
  },
  setpictureRecived(pictureRecived) {
    store.pictureRecived = pictureRecived;
  },
  setMarker(x, y, markerNumber) {
    store.markers[markerNumber].x = x;
    store.markers[markerNumber].y = y;
  },
  setmapOffSetX(mapOffSetX) {
    store.mapOffSetX = mapOffSetX;
  },
  setmapOffSetY(mapOffSetY) {
    store.mapOffSetY = mapOffSetY;
  },
  setstartHeight(startHeight){
    store.startHeight = startHeight;
  },
  setstartHeight(stopHeight){
    store.stopHeight = stopHeight;
  }
};
