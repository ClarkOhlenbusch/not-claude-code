from flask import Flask, jsonify, request

app = Flask(__name__)

# In-memory storage: list of dicts
todos = []
next_id = 1


@app.route("/todos", methods=["GET"])
def get_todos():
    return jsonify(todos), 200


@app.route("/todos", methods=["POST"])
def create_todo():
    global next_id
    data = request.get_json()
    if not data or "title" not in data:
        return jsonify({"error": "title is required"}), 400
    todo = {"id": next_id, "title": data["title"]}
    next_id += 1
    todos.append(todo)
    return jsonify(todo), 201


@app.route("/todos/<int:todo_id>", methods=["DELETE"])
def delete_todo(todo_id):
    global todos
    original_len = len(todos)
    todos = [t for t in todos if t["id"] != todo_id]
    if len(todos) == original_len:
        return jsonify({"error": "not found"}), 404
    return "", 204


if __name__ == "__main__":
    app.run(debug=True)
