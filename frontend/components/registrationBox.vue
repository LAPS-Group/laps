<template>
  <div>
    <h2>Logged in as {{ user.username }}</h2>
    <div v-if="user.isSuper == 1">
      <h2>Register a new admin</h2>
      <label
        >Username
        <input
          type="text"
          name="username"
          v-model="input.username"
          placeholder="Username"
      /></label>
      <br />
      <label
        >Password
        <input
          type="password"
          name="password"
          v-model="input.password"
          placeholder="Password"
          v_on:input="verifyInput"
      /></label>
      <br />
      <label
        >Confirm
        <input
          type="password"
          name="password"
          v-model="input.confirm"
          placeholder="Password"
          v-on:input="verifyInput"
      /></label>
      <br />
      {{ matchMessage }}
      <br />
      <button type="button" v-on:click="register()">Register admin</button>
    </div>
  </div>
</template>

<script>
import { getRoute } from "route";
import axios from "axios";

export default {
  data() {
    return {
      input: {
        username: "",
        password: "",
        confirm: "",
      },
      user: {
        isSuper: false,
        username: "",
      },
      matchMessage: "",
    };
  },
  beforeMount: async function () {
    let me = await axios.get(getRoute("/admin/me"));
    this.user.isSuper = me.data.is_super;
    this.user.username = me.data.username;
  },
  methods: {
    register: async function () {
      if (
        this.input.username !== "" &&
        this.input.password !== "" &&
        this.input.password === this.input.confirm
      ) {
        let url = getRoute("/register");
        const params = new URLSearchParams();
        params.append("username", this.input.username);
        params.append("password", this.input.password);
        axios
          .post(url, params)
          .then(function (data) {
            alert("Registered admin" + this.inuput.username);
          })
          .catch(function (err) {
            alert("Login failed " + err);
          });
      }
    },
    verifyInput() {
      if (this.input.password === this.input.confirm) {
        this.matchMessage = "Passwords match!";
      } else {
        this.matchMessage = "Passwords do not match";
      }
    },
  },
};
</script>
