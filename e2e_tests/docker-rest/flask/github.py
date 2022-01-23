import os.path

from app import app

from flask import Flask, request, abort, jsonify, make_response

import jinja2


def check_headers():
    if request.headers.get("accept") != "application/vnd.github.v3+json":
        app.logger.error("Invalid accept header")
        abort(500)
    auth_header = request.headers.get("authorization")
    if auth_header != "token authtoken":
        app.logger.error("Invalid authorization header: %s", auth_header)
        abort(
            make_response(
                jsonify(
                    {
                        "message": "Bad credentials",
                        "documentation_url": "https://docs.example.com/rest",
                    }
                ),
                401,
            )
        )


def add_pagination(response, page, last_page):
    host = request.headers["host"]
    link_header = ""

    def args(page):
        args = request.args.copy()
        args["page"] = page
        return "&".join([f"{k}={v}" for k, v in args.items()])

    if page < last_page:
        link_header += (
            f'<{request.scheme}://{host}{request.path}?{args(page+1)}>; rel="next", '
        )
    link_header += (
        f'<{request.scheme}://{host}{request.path}?{args(last_page)}>; rel="last"'
    )
    response.headers["link"] = link_header


def read_project_files(namespaces=[]):
    last_page = 4
    page = username = int(request.args.get("page", "1"))
    response_file = f"./github_api_page_{page}.json.j2"
    if not os.path.exists(response_file):
        return jsonify([])

    response = make_response(
        jinja2.Template(open(response_file).read()).render(
            namespace=namespaces[page - 1]
        )
    )
    add_pagination(response, page, last_page)
    response.headers["content-type"] = "application/json"
    return response


def single_namespaced_projects(namespace):
    return read_project_files([namespace] * 4)


def mixed_projects(namespaces):
    return read_project_files(namespaces)


@app.route("/github/users/<string:user>/repos/")
def github_user_repos(user):
    check_headers()
    if user == "myuser1":
        return single_namespaced_projects("myuser1")
    return jsonify([])


@app.route("/github/orgs/<string:group>/repos/")
def github_group_repos(group):
    check_headers()
    if not (request.args.get("type") == "all"):
        abort(500, "wrong arguments")
    if group == "mygroup1":
        return single_namespaced_projects("mygroup1")
    return jsonify([])


@app.route("/github/user/repos/")
def github_own_repos():
    check_headers()
    return mixed_projects(["myuser1", "myuser2", "mygroup1", "mygroup2"])


@app.route("/github/user/")
def github_user():
    check_headers()
    response = make_response(open("./github_api_user.json").read())
    response.headers["content-type"] = "application/json"
    return response
