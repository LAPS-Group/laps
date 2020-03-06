import Vue from "vue";
//store variables
export const store = Vue.observable({
  tester: 0,
  recivedCoordinates: {
    test: "noe"
  }
});

//function to update a variable
export const mutations = {
  settester(tester) {
    store.tester = tester;
  },
  setrecivedCoordinates(recivedCoordinates) {
    store.recivedCoordinates = recivedCoordinates;
  }
};
