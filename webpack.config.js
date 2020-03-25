const HtmlWebpackPlugin = require("html-webpack-plugin");
const path = require("path");
const VueLoaderPlugin = require("vue-loader/lib/plugin");

module.exports = env => {
  let routeAlias;
  if (env.production) {
    console.log("Running in production mode");
    routeAlias = "./frontend/route_production.js";
  } else {
    console.log("Not using production");
    routeAlias = "./frontend/route.js";
  }

  return {
    entry: "./frontend/index.js",
    output: {
      filename: "main.js",
      path: path.resolve(__dirname, "dist")
    },
    devServer: {
      disableHostCheck: true,
      headers: {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Credentials': 'true',
        'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, PATCH, OPTIONS',
        'Access-Control-Allow-Headers': 'X-Requested-With, content-type, Authorization'
      }
    },
    resolve: {
      alias: {
        vue$: "vue/dist/vue.esm.js",
        route: path.resolve(__dirname, routeAlias)
      }
    },
    module: {
      rules: [
        // ... other rules
        {
          test: /\.vue$/,
          loader: "vue-loader"
        },
        {
          test: /\.css$/,
          use: ["vue-style-loader", "css-loader"]
        },
        {
          test: /\.(png|jpe?g|gif)$/i,
          use: [
            {
              loader: "file-loader",
            },
          ],
        },
        {
          test: /\.scss$/,
          use: ['vue-style-loader','css-loader','sass-loader']
        },
        {
          test: /\.(html)$/,
          use: ['html-loader']
       }
      ]
    },
    plugins: [
      new HtmlWebpackPlugin({
        template: './src/index.html'
      }),
      new VueLoaderPlugin(),
      new HtmlWebpackPlugin({
        filename: "index.html",
        template: "frontend/index.html",
        
      })
    ]
  };
};
