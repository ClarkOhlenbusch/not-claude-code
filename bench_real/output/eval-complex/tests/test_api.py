import pytest
from app import app, todos

@pytest.fixture
def client():
    app.config['TESTING'] = True
    with app.test_client() as client:
        # Reset in-memory storage for each test
        import app as app_module
        app_module.todos = []
        app_module.next_id = 1
        yield client

def test_create_and_get_todo(client):
    # 1. Create a todo via POST
    payload = {"title": "Buy groceries"}
    response = client.post("/todos", json=payload)
    assert response.status_code == 201
    data = response.get_json()
    assert data["title"] == "Buy groceries"
    assert "id" in data

    # 2. Verify it appears in GET
    response = client.get("/todos")
    assert response.status_code == 200
    todos_list = response.get_json()
    assert len(todos_list) == 1
    assert todos_list[0]["title"] == "Buy groceries"

def test_create_and_delete_todo(client):
    # 1. Create two todos
    client.post("/todos", json={"title": "Task 1"})
    client.post("/todos", json={"title": "Task 2"})
    
    # Get them to find IDs
    response = client.get("/todos")
    todos_list = response.get_json()
    id_to_delete = todos_list[0]["id"]
    
    # 2. Delete one
    response = client.delete(f"/todos/{id_to_delete}")
    assert response.status_code == 204
    
    # 3. Verify only the other remains
    response = client.get("/todos")
    todos_list = response.get_json()
    assert len(todos_list) == 1
    assert todos_list[0]["title"] == "Task 2"

def test_delete_non_existent_todo(client):
    # Attempt to delete a non-existent id
    response = client.delete("/todos/999")
    assert response.status_code == 404
