use openapi3::OpenApi;
use openapi3::objects::*;
use errors::ErrorKind;
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use Result;
use inflector::Inflector;

#[derive(Debug, Clone)]
pub struct Entrypoint<'a> {
    route: Route<'a>,
    pub method: Method,
    pub args: Vec<Arg>,
    pub responses: Vec<Response>,
    pub operation_id: OperationId,
    pub summary: Option<String>,
    pub description: Option<String>,
}

pub fn extract_entrypoints(spec: &OpenApi) -> Vec<Entrypoint> {
    let mut out = Vec::new();
    let mut components = &Default::default();
    components = spec.components.as_ref().unwrap_or(components);
    for (route, path) in &spec.paths {
        for (method, op) in path_as_map(path) {
            match Entrypoint::build(route, method, op, components) {
                Ok(entrypoint) => out.push(entrypoint),
                // TODO better error handling
                Err(e) => eprintln!("{}", e),
            }
        }
    }
    out
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OperationId(String);

impl OperationId {
    fn new(s: &str) -> OperationId {
        OperationId(s.to_snake_case())
    }

    fn classcase(&self) -> String {
        self.0.to_class_case()
    }
}

impl<'a> Entrypoint<'a> {
    fn new(
        route: Route<'a>,
        method: Method,
        args: Vec<Arg>,
        responses: Vec<Response>,
        operation_id: OperationId,
        summary: Option<String>,
        description: Option<String>,
    ) -> Result<Self> {
        validate_route_args(&route, &args)?;
        Ok(Entrypoint {
            route,
            method,
            args,
            responses,
            operation_id,
            summary,
            description,
        })
    }

    fn build(
        route: &'a str,
        method: Method,
        operation: &Operation,
        components: &Components,
    ) -> Result<Entrypoint<'a>> {
        let args = build_args(operation, components)?;
        let responses = build_responses(operation, components);
        let responses = responses
            .into_iter()
            .filter_map(|res| match res {
                Ok(resp) => Some(resp),
                Err(e) => {
                    // TODO better error handling
                    eprintln!("{}", e);
                    None
                }
            })
            .collect();
        let operation_id = operation
            .operation_id
            .as_ref()
            .ok_or(ErrorKind::from("No operation_id found"))?;
        Entrypoint::new(
            Route::from_str(&route)?,
            method,
            args,
            responses,
            OperationId::new(operation_id),
            operation.summary.clone(),
            operation.description.clone(),
        )
    }

    pub fn build_template_args(&self) -> JsonValue {
        let (args_json, anon_count) = self.args.iter().fold(
            (Vec::new(), 1),
            |(mut out, anon_count), arg| {
                let rendered_type = arg.type_.render(anon_count, &self.operation_id);
                let json = json!({
                "name": arg.name,
                "type": rendered_type.0
            });
                out.push(json);
                (out, rendered_type.1)
            },
        );
        json!({
            "method": self.method,
            "route": self.route.render(),
            // TODO verify that operation_id is valid
            "function": self.operation_id,
            "args": args_json,
            "result_type": self.result_type(anon_count),
            "documentation": self.docstring()
        })
    }

    fn docstring(&self) -> Option<String> {
        match (self.summary.as_ref(), self.description.as_ref()) {
            (Some(s), Some(d)) => Some(format!("/// {}\n/// {}\n", s, d)), // show both
            (Some(s), None) => Some(format!("/// {}\n", s)),
            (None, Some(ref d)) => Some(format!("/// {}\n", d)),
            (None, None) => None,
        }
    }

    fn result_type(&self, anon_count: u32) -> String {
        // just takes the first response type in the 200 range
        match self.responses
            .iter()
            .filter(|resp| resp.status_code.starts_with("2"))
            .next() {
            Some(ref resp) => {
                match resp.return_type {
                    Some(ref type_) => type_.render(anon_count, &self.operation_id).0,
                    None => "()".into(),
                }
            }
            None => {
                eprintln!("Warning: no success code found");
                "()".into()
            }
        }
    }

    pub fn swagger_entrypoint() -> Entrypoint<'a> {
        Entrypoint::new(
            Route::from_str("/swagger".into()).unwrap(),
            Method::Get,
            Vec::new(),
            vec![Response::new("200".into(),
                               Some(NativeType::String),
                               Some("application/json".into()))],
            OperationId::new("getSwagger"),
            Some("OpenAPI schema in JSON format".into()),
            None,
        ).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Arg {
    name: String,
    pub type_: NativeType,
    pub location: Location,
}

impl Arg {
    fn new(name: &str, type_: NativeType, location: Location) -> Self {
        Self {
            name: name.to_snake_case(),
            type_,
            location,
        }
    }
}

impl Arg {
    fn build_from_parameter(parameter: &Parameter) -> Result<Arg> {
        let required = parameter.required.unwrap_or(false);
        let native_type = NativeType::from_json_schema(&parameter.schema, required)?;
        Ok(Arg::new(&parameter.name, native_type, parameter.in_))
    }
}

fn build_args(operation: &Operation, components: &Components) -> Result<Vec<Arg>> {
    let op_parameters = match operation.parameters.as_ref() {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };
    op_parameters
        .iter()
        .map(|maybe| {
            maybe
                .resolve_ref_opt(&components.parameters)
                .map_err(|e| e.into())
                .and_then(Arg::build_from_parameter)
        })
        .collect()
}

#[derive(Debug, Default, Clone, new)]
pub struct Response {
    pub status_code: String,
    pub return_type: Option<NativeType>,
    pub content_type: Option<String>,
}

impl Response {
    fn build_from_response_obj(
        status_code: String,
        response_obj: &ResponseObj,
    ) -> Result<Response> {
        match response_obj.content {
            None => return Ok(Response::new(status_code, None, None)), // No data returned
            Some(ref content_map) => {
                content_map
                    .iter()
                    .next()
                    .ok_or("Content map empty".into())
                    .and_then(|(content_type, media)| {
                        media
                                .schema
                                .as_ref()
                                .ok_or("Media schema not found".into())
                                // For responses, the default required state is 'true'
                                .and_then(|maybe| NativeType::from_json_schema(maybe, true))
                                .map(|typ| {
                                    Response::new(
                                        status_code,
                                        Some(typ),
                                        Some(content_type.clone()),
                                    )
                                })
                    })
            }
        }
    }
}

fn build_responses(operation: &Operation, components: &Components) -> Vec<Result<Response>> {
    operation
        .responses
        .iter()
        .map(|(code, maybe)| {
            let response_obj = maybe.resolve_ref_opt(&components.responses)?;
            Response::build_from_response_obj(code.clone(), response_obj)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeType {
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Named(String),
    Array(Vec<NativeType>),
    Option(Box<NativeType>),
    Anonymous(Box<Schema>),
}

impl NativeType {
    fn from_json_schema(schema: &Schema, required: bool) -> Result<Self> {
        let out = if let Some(ref ref_) = schema.ref_ {
            // If the schema is a reference, grab the name
            match ref_.rfind("/") {
                None => bail!("Reference {} is not valid path", ref_),
                Some(loc) => {
                    let refname = ref_.split_at(loc + 1).1;
                    NativeType::Named(refname.into())
                }
            }
        } else {
            match schema.type_.len() {
                0 => NativeType::Anonymous(Box::new(schema.clone())), // assume it is an object
                1 => {
                    // If the type is a primitive, pluck it from the schema
                    // Otherwise, return the schema
                    use openapi3::objects::SimpleTypes::*;
                    match *(schema.type_.first().unwrap()) {
                        Object => NativeType::Anonymous(Box::new(schema.clone())),
                        Boolean => NativeType::Bool,
                        Integer => NativeType::I64,
                        Null => bail!("Null is not valid as per spec"),
                        Number => NativeType::F64,
                        String => NativeType::String,
                        Array => {
                            if schema.items.len() == 0 {
                                bail!("Items missing for array schema")
                            }
                            let natives = schema
                                .items
                                .iter()
                                .map(|schema| NativeType::from_json_schema(schema, required))
                                .collect::<Result<Vec<_>>>()?;
                            NativeType::Array(natives)
                        }
                    }
                }
                other => bail!("Schema type is array of len {}", other),
            }
        };
        if !required {
            Ok(NativeType::Option(Box::new(out)))
        } else {
            Ok(out)
        }
    }

    fn render(&self, mut anon_count: u32, operation_id: &OperationId) -> (String, u32) {
        use self::NativeType::*;
        let res = match *self {
            I32 => "i32".into(),
            I64 => "i64".into(),
            F32 => "f32".into(),
            F64 => "f64".into(),
            Bool => "bool".into(),
            String => "String".into(),
            Named(ref s) => s.clone(),
            Array(ref natives) => {
                let rendered_type = natives.first().unwrap().render(anon_count, operation_id);
                anon_count = rendered_type.1;
                format!("Vec<{}>", rendered_type.0)
            }
            Option(ref native) => {
                let rendered_type = native.render(anon_count, operation_id);
                anon_count = rendered_type.1;
                format!("Option<{}>", rendered_type.0)
            }
            Anonymous(_) => {
                anon_count += 1;
                format!("{}AnonArg{}", operation_id.classcase(), anon_count - 1)
            }
        };
        (res, anon_count)
    }
}


fn path_as_map(path: &Path) -> BTreeMap<Method, &Operation> {
    use self::Method::*;
    let mut map = BTreeMap::new();
    if let Some(ref op) = path.get {
        map.insert(Get, op);
    }
    if let Some(ref op) = path.post {
        map.insert(Post, op);
    }
    if let Some(ref op) = path.put {
        map.insert(Put, op);
    }
    if let Some(ref op) = path.patch {
        map.insert(Patch, op);
    }
    if let Some(ref op) = path.delete {
        map.insert(Delete, op);
    }
    map
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum RouteSegment<'a> {
    Path(&'a str),
    RouteArg(&'a str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Route<'a>(Vec<RouteSegment<'a>>);

impl<'a> Route<'a> {
    fn from_str(route: &str) -> Result<Route> {
        // TODO reinventing the wheel here?

        fn is_valid(section: &str) -> bool {
            !(section.contains('{') || section.contains('}'))
        }

        let re_route_arg = Regex::new(r"^\{(.+)\}$").unwrap();
        let segments = route
            .split("/")
            .map(|segment| {
                re_route_arg
                    .captures(segment)
                    .map(|c| c.get(1).unwrap().as_str())
                    .map(|s| match is_valid(s) {
                        true => Ok(RouteSegment::RouteArg(s)),
                        false => bail!("Invalid segment: {}", s),
                    })
                    .unwrap_or_else(|| match is_valid(segment) {
                        true => Ok(RouteSegment::Path(segment)),
                        false => bail!("Invalid segment: {}", segment),
                    })
            })
            .collect::<Result<Vec<RouteSegment>>>()?;
        Ok(Route(segments))
    }

    fn render(&self) -> String {
        self.0
            .iter()
            .map(|section| match *section {
                RouteSegment::Path(path) => path.into(),
                RouteSegment::RouteArg(route_arg) => format!("<{}>", route_arg.to_snake_case()),
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    fn route_args(&self) -> Vec<String> {
        self.0
            .iter()
            .filter_map(|ra| match *ra {
                RouteSegment::RouteArg(ref a) => Some(a.to_snake_case()),
                _ => None,
            })
            .collect()
    }
}


fn validate_route_args(route: &Route, args: &Vec<Arg>) -> Result<()> {
    let mut route_args = route.route_args();
    let mut path_args: Vec<&str> = args.iter()
        .filter_map(|arg| if arg.location == Location::Path {
            Some(arg.name.as_str())
        } else {
            None
        })
        .collect();
    route_args.sort();
    path_args.sort();
    if !(route_args == path_args) {
        bail!("Path args mismatch - expected {:?}, found {:?}", route_args, path_args)
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_parse_route_args() {
        use self::RouteSegment::*;
        let res = Route::from_str("/pets/{petId}/name/{petName}").unwrap();
        let expect = vec![Path(""), Path("pets"), RouteArg("petId"), Path("name"),
                          RouteArg("petName")];
        assert_eq!(res.0, expect);

        assert!(Route::from_str("/pets/{petId}/name/x{bogus}x").is_err());
        assert!(Route::from_str("/pets/{petId}/name/x{bogus}").is_err());
        assert!(Route::from_str("/pets/{petId}/name/{bogus}x").is_err());
    }

    #[test]
    fn test_extract_entrypoints() {
        // TODO test contents of entrypoints
        let yaml = include_str!("../example_apis/petstore.yaml");
        let api = OpenApi::from_string(yaml).unwrap();
        let entrypoints = extract_entrypoints(&api);
        assert_eq!(entrypoints.len(), 3);
    }

    #[test]
    fn test_atom_schemafy() {
        let schema = r#"{"type": "integer"}"#;
        let schema: Schema = serde_json::from_str(schema).unwrap();
        let outcome = schema.generate_code("my dummy type".into()).unwrap();
        println!("{}", outcome);
        assert!(outcome.contains("MyDummyType = i64"));
    }

    #[test]
    fn test_simple_schemafy() {
        let yaml = include_str!("../example_apis/petstore.yaml");
        let api = OpenApi::from_string(yaml).unwrap();
        let schema: &Schema = api.components
            .as_ref()
            .unwrap()
            .schemas
            .as_ref()
            .unwrap()
            .get("Pet")
            .unwrap(); // yuck
        let native = NativeType::from_json_schema(&schema, true).unwrap();
        // TODO: this would be easier if Schema had a default impl
        let expectstr = r#"{
            "required": [ "id", "name" ],
            "properties": {
                "id": { "type": "integer", "format": "int64" },
                "name": { "type": "string" },
                "tag": { "type": "string" }
            }
        }"#;
        let expect_schema: Schema = serde_json::from_str(expectstr).unwrap();
        assert_eq!(native, NativeType::Anonymous(Box::new(expect_schema)));
    }

    #[test]
    fn test_referenced_schemafy() {
        let yaml = include_str!("../example_apis/petstore.yaml");
        let api = OpenApi::from_string(yaml).unwrap();
        let schema: &Schema = api.components
            .as_ref()
            .unwrap()
            .schemas
            .as_ref()
            .unwrap()
            .get("Pets")
            .unwrap(); // yuck
        let native = NativeType::from_json_schema(&schema, true).unwrap();
        let expect = NativeType::Array(vec![NativeType::Named("Pet".into())]);
        assert_eq!(native, expect);
    }

    #[test]
    fn test_entrypoint_render() {

        fn make_entrypoint<'a>(routestr: &'a str) -> Result<Entrypoint<'a>> {
            let inner_schema: Schema = serde_json::from_value(json!({
                "properties": {
                    "some type": {"type": "integer"},
                    "some other type": {"type": "number"}
                }
            })).unwrap();
            let mut args = vec![
                Arg::new(
                    "arg_one".into(),
                    NativeType::Anonymous(Box::new(inner_schema.clone())),
                    Location::Path),
                Arg::new(
                    "arg_two".into(),
                    NativeType::Anonymous(Box::new(inner_schema.clone())),
                    Location::Path),
                Arg::new(
                    // TODO this should fail with duplicate arg
                    "ArgOne".into(),
                    NativeType::Anonymous(Box::new(inner_schema.clone())),
                    Location::Query),
            ];
            let responses = vec![
                Response::new(
                    "200".into(),
                    None,
                    None)
            ];
            Entrypoint::new(
                Route::from_str(routestr).unwrap(),
                Method::Post,
                args,
                responses,
                OperationId::new("my operation id"),
                None,
                Some("some description".into()))
        }

        let route1 = "/this/{argOne}/is/a/route";
        let route2 = "/this/{argOne}/{ArgTwo}/a/route";
        let route3 = "/this/{argOne}/{ArgTwo}/{arg_three}/route";
        assert!(make_entrypoint(route1).is_err());
        let entrypoint = make_entrypoint(route2).unwrap();
        assert!(make_entrypoint(route3).is_err());
    }
}
