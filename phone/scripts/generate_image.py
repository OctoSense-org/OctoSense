import base64
from pathlib import Path
from openai import OpenAI

client = OpenAI()  # Reads OPENAI_API_KEY

response = client.images.generate(
    model="gpt-image-2.5-flare",
    prompt="generate 6 different persons. they are part of a family. their roles are father, mother, son, daughter, grandfather and grandmother",
    size="1024x1024",
    quality="medium",
    output_format="png",
)

image_bytes = base64.b64decode(response.data[0].b64_json)
Path("generated.png").write_bytes(image_bytes)
print("Saved generated.png")
