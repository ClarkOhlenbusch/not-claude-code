import csv
import json
import os

script_dir = os.path.dirname(os.path.abspath(__file__))
csv_path = os.path.join(script_dir, "data.csv")
json_path = os.path.join(script_dir, "data.json")

with open(csv_path, newline="") as f:
    reader = csv.DictReader(f)
    rows = []
    for row in reader:
        row["age"] = int(row["age"])
        rows.append(row)

with open(json_path, "w") as f:
    json.dump(rows, f, indent=2)

print(f"Wrote {len(rows)} records to {json_path}")
